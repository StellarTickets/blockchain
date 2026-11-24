//! `active_supply` and the lifetime capacity semantics (issue #511).
//!
//! `TicketsIssued` and tier capacity are lifetime counters. A refunded or
//! revoked ticket keeps its seat, so the tier cannot be resold. `active_supply`
//! is the circulating count: issued minus refunded and revoked tickets.

use super::*;

fn ga(env: &Env) -> String {
    String::from_str(env, "GA")
}

/// A free event with a refund window, so `purchase_primary` tickets can be
/// refunded without any escrow payment.
fn refundable_free_event(env: &Env, client: &TicketingContractClient, organizer: &Address) {
    client.create_event(
        organizer,
        &1,
        &EventParams {
            name: String::from_str(env, "Refundable Show"),
            category: String::from_str(env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 0,
            starts_at: 999_999,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: Some(999_999),
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    client.set_tier_price(organizer, &1, &ga(env), &0i128);
}

fn buy(env: &Env, client: &TicketingContractClient) -> (Address, u64) {
    let buyer = Address::generate(env);
    let ticket_id = client.purchase_primary(&buyer, &1, &ga(env), &String::from_str(env, "A1"));
    (buyer, ticket_id)
}

#[test]
fn refund_lowers_active_supply_but_not_tickets_issued() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    refundable_free_event(&env, &client, &organizer);
    let (_first_buyer, _first) = buy(&env, &client);
    let (refunded_buyer, refunded) = buy(&env, &client);

    client.refund_primary(&refunded_buyer, &refunded);

    assert_eq!(client.get_ticket(&refunded).status, TicketStatus::Refunded);
    assert_eq!(client.get_event(&1).tickets_issued, 2);
    assert_eq!(client.active_supply(&1), 1);
}

#[test]
fn revoke_lowers_active_supply() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let revoked = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.revoke_ticket(&organizer, &revoked);

    assert_eq!(client.get_event(&1).tickets_issued, 2);
    assert_eq!(client.active_supply(&1), 1);
}

#[test]
fn revoke_batch_lowers_active_supply_once_per_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let a = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let b = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.revoke_batch(&organizer, &Vec::from_array(&env, [a, b]));

    assert_eq!(client.get_event(&1).tickets_issued, 3);
    assert_eq!(client.active_supply(&1), 1);
}

#[test]
fn a_refunded_seat_stays_counted_against_tier_capacity() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    refundable_free_event(&env, &client, &organizer);
    client.set_tier_capacity(&organizer, &1, &ga(&env), &Some(1u64));
    let (buyer, ticket_id) = buy(&env, &client);
    client.refund_primary(&buyer, &ticket_id);

    // The refunded seat is not released, so the tier stays sold out.
    let result = client.try_purchase_primary(
        &Address::generate(&env),
        &1,
        &ga(&env),
        &String::from_str(&env, "A2"),
    );

    assert_eq!(result, Err(Ok(Error::TierSoldOut)));
    assert_eq!(client.active_supply(&1), 0);
}

#[test]
fn active_supply_of_an_unknown_event_is_not_found() {
    let (_env, client, _token, _token_asset, _admin, _organizer) = setup();

    assert_eq!(client.try_active_supply(&99), Err(Ok(Error::EventNotFound)));
}
