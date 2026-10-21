//! Resale multiplier validation for `create_event` (Issue #124).
//!
//! A `max_resale_multiplier_bps` below face value (10_000 bps) makes every
//! resale impossible: `list_for_resale` caps the listing at
//! `original_price * max_resale_multiplier_bps / 10_000`, which sits below the
//! face value, so no listing at or above face value can ever be accepted.
//! The contract now rejects such an event up front with
//! `Error::InvalidMultiplier`.

use super::*;

#[test]
fn create_event_rejects_multiplier_below_face_value() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    for bps in [0u32, 1, 5_000, 9_999] {
        let result = client.try_create_event(
            &organizer,
            &1,
            &EventParams {
                name: String::from_str(&env, "Below Face Value"),
                category: String::from_str(&env, "concert"),
                max_resale_multiplier_bps: bps,
                royalty_bps: 500,
                starts_at: 10_000,
                transfer_freeze_seconds: 100,
                resale_cutoff_seconds: 200,
                max_per_wallet: None,
                refund_window_seconds: None,
                presale_starts_at: None,
                presale_ends_at: None,
            },
        );
        assert_eq!(result, Err(Ok(Error::InvalidMultiplier)));
    }
}

#[test]
fn create_event_accepts_multiplier_exactly_at_face_value() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    client.create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Face Value Only"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 10_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );

    let event = client.get_event(&1);
    assert_eq!(event.max_resale_multiplier_bps, 10_000);
}

#[test]
fn create_event_accepts_multiplier_above_face_value() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    client.create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Above Face Value"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );

    let event = client.get_event(&1);
    assert_eq!(event.max_resale_multiplier_bps, 12_000);
}

#[test]
fn face_value_multiplier_permits_resale_at_face_value() {
    // Proves the fix removes the dead-end: with a 10_000 bps multiplier a
    // ticket listed at exactly face value is accepted.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );

    client.list_for_resale(&buyer, &ticket_id, &1_000i128);
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Resale);
    assert_eq!(ticket.resale_price, 1_000);
}

#[test]
fn create_event_with_options_rejects_multiplier_below_face_value() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_create_event_with_options(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Below Face Value"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 5_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::InvalidMultiplier)));
}
