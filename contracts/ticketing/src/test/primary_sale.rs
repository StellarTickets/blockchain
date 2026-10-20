//! `PrimarySale` event on `purchase_primary` (issue #118).
//!
//! A primary sale mints a ticket and moves money, so indexers need the buyer
//! and the price paid. These tests compare the exact XDR event the contract
//! publishes.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

#[test]
fn purchase_primary_publishes_primary_sale_with_buyer_and_price() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);

    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    let expected = PrimarySale {
        ticket_id,
        buyer: buyer.clone(),
        price: 2_000,
    }
    .to_xdr(&env, &client.address);
    let published = env
        .events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count();
    assert_eq!(published, 1);
}

#[test]
fn rejected_purchase_publishes_no_primary_sale() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    // No tier price is set, so the purchase is rejected before any mint.
    let events_before = env.events().all().events().len();

    let result = client.try_purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );
    assert_eq!(result, Err(Ok(Error::TierPriceNotSet)));

    assert_eq!(env.events().all().events().len(), events_before);
}
