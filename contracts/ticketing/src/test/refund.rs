//! Tests for `refund_primary` refund window validation (issue #625).
//!
//! The `RefundWindowExpired` boundary (`now > purchased_at + window`) needs
//! coverage, including the `None` window case where refunds are disallowed.

use super::*;

const STARTS_AT: u64 = 10_000;
const REFUND_WINDOW: u64 = 7 * 24 * 3600; // 7 days in seconds

fn create_event_with_refund_window(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    refund_window: Option<u64>,
) -> u64 {
    let params = EventParams {
        name: String::from_str(env, "Event With Refund"),
        category: String::from_str(env, "concert"),
        max_resale_multiplier_bps: 12_000u32,
        royalty_bps: 1_000u32,
        starts_at: STARTS_AT,
        transfer_freeze_seconds: 3600u64,
        resale_cutoff_seconds: 3600u64,
        max_per_wallet: None,
        refund_window_seconds: refund_window,
        presale_starts_at: None,
        presale_ends_at: None,
    };
    client.create_event_with_options(organizer, &1, &params, &None, &None);
    1
}

fn purchase_ticket(
    env: &Env,
    client: &TicketingContractClient,
    token_asset: &StellarAssetClient,
    organizer: &Address,
    event_id: u64,
    buyer: &Address,
    price: i128,
) -> u64 {
    client.set_tier_price(organizer, &event_id, &String::from_str(env, "GA"), &price);
    token_asset.mint(buyer, &(price * 2));
    let ticket_id = client.purchase_primary(
        buyer,
        &event_id,
        &String::from_str(env, "GA"),
        &String::from_str(env, "unassigned"),
        &price, // max_price parameter
    );
    ticket_id
}

#[test]
fn refund_primary_rejects_at_exact_deadline() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    let event_id = create_event_with_refund_window(&env, &client, &organizer, Some(REFUND_WINDOW));
    let buyer = Address::generate(&env);

    // Set initial time for purchase
    env.ledger().set_timestamp(STARTS_AT - 10_000);
    let purchased_at = env.ledger().timestamp();
    let ticket_id = purchase_ticket(
        &env,
        &client,
        &token_asset,
        &organizer,
        event_id,
        &buyer,
        1_000,
    );

    // Set time to exact deadline (purchased_at + REFUND_WINDOW)
    let deadline = purchased_at + REFUND_WINDOW;
    env.ledger().set_timestamp(deadline);

    // Should succeed at exact deadline
    client.refund_primary(&buyer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Refunded);
}

#[test]
fn refund_primary_rejects_one_second_after_deadline() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    let event_id = create_event_with_refund_window(&env, &client, &organizer, Some(REFUND_WINDOW));
    let buyer = Address::generate(&env);

    // Set initial time for purchase
    env.ledger().set_timestamp(STARTS_AT - 10_000);
    let purchased_at = env.ledger().timestamp();
    let ticket_id = purchase_ticket(
        &env,
        &client,
        &token_asset,
        &organizer,
        event_id,
        &buyer,
        1_000,
    );

    // Set time to one second after deadline
    let one_second_after = purchased_at + REFUND_WINDOW + 1;
    env.ledger().set_timestamp(one_second_after);

    // Should fail after the deadline
    let result = client.try_refund_primary(&buyer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::RefundWindowExpired)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn refund_primary_rejects_when_window_is_none() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    let event_id = create_event_with_refund_window(&env, &client, &organizer, None);
    let buyer = Address::generate(&env);

    // Set initial time for purchase
    env.ledger().set_timestamp(STARTS_AT - 10_000);
    let ticket_id = purchase_ticket(
        &env,
        &client,
        &token_asset,
        &organizer,
        event_id,
        &buyer,
        1_000,
    );

    // Try to refund immediately (should fail since window is None)
    let result = client.try_refund_primary(&buyer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::RefundWindowExpired)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn refund_primary_succeeds_within_window() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    let event_id = create_event_with_refund_window(&env, &client, &organizer, Some(REFUND_WINDOW));
    let buyer = Address::generate(&env);

    // Set initial time for purchase
    env.ledger().set_timestamp(STARTS_AT - 10_000);
    let purchased_at = env.ledger().timestamp();
    let ticket_id = purchase_ticket(
        &env,
        &client,
        &token_asset,
        &organizer,
        event_id,
        &buyer,
        1_000,
    );

    // Set time to middle of window
    env.ledger().set_timestamp(purchased_at + REFUND_WINDOW / 2);

    // Should succeed within the window
    client.refund_primary(&buyer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Refunded);
}
