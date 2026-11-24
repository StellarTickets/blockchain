//! `TierPriceSet` event on `set_tier_price` (issue #514).
//!
//! Price changes used to be silent, so buyers and the backend could not audit
//! what was charged when. Every accepted price change now publishes an event,
//! and rejected changes publish nothing. These tests compare the exact XDR
//! event the contract publishes.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

fn count_tier_price_set(
    env: &Env,
    client: &TicketingContractClient,
    event_id: u64,
    tier: &str,
    price: i128,
) -> usize {
    let expected = TierPriceSet {
        event_id,
        tier: String::from_str(env, tier),
        price,
    }
    .to_xdr(env, &client.address);
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count()
}

#[test]
fn set_tier_price_publishes_tier_price_set() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &100);

    assert_eq!(count_tier_price_set(&env, &client, 1, "GA", 100), 1);
}

#[test]
fn each_price_change_publishes_its_own_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let tier = String::from_str(&env, "GA");

    client.set_tier_price(&organizer, &1, &tier, &100);
    client.set_tier_price(&organizer, &1, &tier, &150);

    // The mid-sale increase is published too, so the history can be rebuilt.
    assert_eq!(count_tier_price_set(&env, &client, 1, "GA", 150), 1);
    assert_eq!(client.get_tier_price(&1, &tier), 150);
}

#[test]
fn setting_a_tier_free_publishes_a_zero_price_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &0);

    assert_eq!(count_tier_price_set(&env, &client, 1, "GA", 0), 1);
}

#[test]
fn rejected_tier_price_change_publishes_nothing() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);

    let negative = client.try_set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &-1);
    assert_eq!(negative, Err(Ok(Error::InvalidPrice)));
    assert_eq!(count_tier_price_set(&env, &client, 1, "GA", -1), 0);

    let unauthorised =
        client.try_set_tier_price(&stranger, &1, &String::from_str(&env, "GA"), &200);
    assert!(unauthorised.is_err());
    assert_eq!(count_tier_price_set(&env, &client, 1, "GA", 200), 0);
}
