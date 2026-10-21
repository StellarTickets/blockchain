//! Per-tier capacity limits (issue #93).
//!
//! Each tier of an event counts the tickets issued into it. An organizer can
//! cap a tier with `set_tier_capacity`, and every mint path rejects a mint that
//! would take the tier past its cap with `TierSoldOut`.

use super::*;

fn tier(env: &Env, name: &str) -> String {
    String::from_str(env, name)
}

fn seat(env: &Env) -> String {
    String::from_str(env, "unassigned")
}

#[test]
fn issue_ticket_stops_when_the_tier_cap_is_reached() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(2u64));
    let owner = Address::generate(&env);

    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_eq!(
        client.try_issue_ticket(
            &organizer,
            &1,
            &owner,
            &tier(&env, "GA"),
            &seat(&env),
            &1_000i128,
        ),
        Err(Ok(Error::TierSoldOut))
    );
    assert_eq!(client.get_event(&1).tickets_issued, 2);
}

#[test]
fn a_full_tier_does_not_block_other_tiers() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(1u64));
    let owner = Address::generate(&env);

    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let vip = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &tier(&env, "VIP"),
        &seat(&env),
        &5_000i128,
    );

    assert_eq!(client.get_ticket(&vip).tier, tier(&env, "VIP"));
}

#[test]
fn purchase_primary_respects_the_tier_cap() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_price(&organizer, &1, &tier(&env, "GA"), &0i128);
    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(1u64));
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    client.purchase_primary(&first, &1, &tier(&env, "GA"), &String::from_str(&env, "A1"));

    assert_eq!(
        client.try_purchase_primary(
            &second,
            &1,
            &tier(&env, "GA"),
            &String::from_str(&env, "A2")
        ),
        Err(Ok(Error::TierSoldOut))
    );
}

#[test]
fn a_batch_that_exceeds_the_tier_cap_is_rejected_whole() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(2u64));
    let mut recipients = Vec::new(&env);
    for _ in 0..3 {
        recipients.push_back(TicketRecipient {
            to: Address::generate(&env),
            tier: tier(&env, "GA"),
            seat: seat(&env),
            price: 1_000,
        });
    }

    assert_eq!(
        client.try_issue_tickets(&organizer, &1, &recipients),
        Err(Ok(Error::TierSoldOut))
    );
    assert_eq!(client.get_event(&1).tickets_issued, 0);
}

#[test]
fn tickets_issued_before_a_cap_is_set_count_toward_it() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_eq!(
        client.try_set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(1u64)),
        Err(Ok(Error::InvalidCapacity))
    );
    // Equal to the issued count leaves no room but is not below it.
    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(2u64));
    assert_eq!(
        client.try_issue_ticket(
            &organizer,
            &1,
            &owner,
            &tier(&env, "GA"),
            &seat(&env),
            &1_000i128
        ),
        Err(Ok(Error::TierSoldOut))
    );
}

#[test]
fn zero_tier_cap_is_rejected() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_eq!(
        client.try_set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(0u64)),
        Err(Ok(Error::InvalidCapacity))
    );
}

#[test]
fn removing_the_tier_cap_lets_the_tier_grow_again() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &Some(1u64));
    let owner = Address::generate(&env);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.set_tier_capacity(&organizer, &1, &tier(&env, "GA"), &None);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_eq!(client.get_event(&1).tickets_issued, 2);
}

#[test]
fn only_the_organizer_can_set_a_tier_cap() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_set_tier_capacity(&stranger, &1, &tier(&env, "GA"), &Some(5u64)),
        Err(Ok(Error::NotOrganizer))
    );
}
