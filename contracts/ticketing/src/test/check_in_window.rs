//! Check-in window (issue #95).
//!
//! `check_in_opens_seconds` opens check-in before `starts_at`, and `ends_at`
//! closes it. Both bounds are checked against the ledger timestamp in
//! `check_in` and `check_in_batch`. Events without a window accept check-in at
//! any time.

use super::*;

/// `make_event` starts events at this timestamp.
const STARTS_AT: u64 = 10_000;

fn issued_ticket(env: &Env, client: &TicketingContractClient, organizer: &Address) -> u64 {
    issue_sample_ticket(env, client, organizer, 1, &Address::generate(env), 1_000)
}

#[test]
fn events_without_a_window_accept_check_in_at_any_time() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issued_ticket(&env, &client, &organizer);

    env.ledger().set_timestamp(0);
    client.check_in(&organizer, &ticket_id);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn check_in_is_rejected_before_the_window_opens() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_check_in_window(&organizer, &1, &Some(100u64), &None);
    let ticket_id = issued_ticket(&env, &client, &organizer);

    env.ledger().set_timestamp(STARTS_AT - 101);
    assert_eq!(
        client.try_check_in(&organizer, &ticket_id),
        Err(Ok(Error::CheckInWindowClosed))
    );

    // The window opens exactly `check_in_opens_seconds` before the start.
    env.ledger().set_timestamp(STARTS_AT - 100);
    client.check_in(&organizer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn check_in_is_rejected_after_the_window_closes() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_check_in_window(&organizer, &1, &None, &Some(STARTS_AT + 500));
    let ticket_id = issued_ticket(&env, &client, &organizer);

    env.ledger().set_timestamp(STARTS_AT + 501);
    assert_eq!(
        client.try_check_in(&organizer, &ticket_id),
        Err(Ok(Error::CheckInWindowClosed))
    );

    // `ends_at` itself is still inside the window.
    env.ledger().set_timestamp(STARTS_AT + 500);
    client.check_in(&organizer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn check_in_batch_is_rejected_outside_the_window() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_check_in_window(&organizer, &1, &None, &Some(STARTS_AT + 500));
    let ticket_id = issued_ticket(&env, &client, &organizer);
    let ticket_ids = soroban_sdk::vec![&env, ticket_id];

    env.ledger().set_timestamp(STARTS_AT + 501);
    assert_eq!(
        client.try_check_in_batch(&organizer, &ticket_ids),
        Err(Ok(Error::CheckInWindowClosed))
    );
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn window_must_end_after_the_event_starts() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_eq!(
        client.try_set_event_check_in_window(&organizer, &1, &None, &Some(STARTS_AT)),
        Err(Ok(Error::InvalidCheckInWindow))
    );
}

#[test]
fn check_in_batch_accepts_the_exact_ends_at_boundary() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_check_in_window(&organizer, &1, &None, &Some(STARTS_AT + 500));
    let ticket_id = issued_ticket(&env, &client, &organizer);
    let ticket_ids = soroban_sdk::vec![&env, ticket_id];

    env.ledger().set_timestamp(STARTS_AT + 500);
    client.check_in_batch(&organizer, &ticket_ids);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}
