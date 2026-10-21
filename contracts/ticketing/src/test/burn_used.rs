//! Burning used tickets after their retention period (issue #113).
//!
//! The retention period runs from the event's `starts_at`. Until it ends, a
//! used ticket's record stays readable. Once it ends, the organizer can burn
//! the record to release its storage.

use super::*;

/// `make_event` creates events starting at this timestamp.
const STARTS_AT: u64 = 10_000;

fn used_ticket(env: &Env, client: &TicketingContractClient, organizer: &Address) -> (Address, u64) {
    make_event(env, client, organizer, 1);
    let owner = Address::generate(env);
    let ticket_id = issue_sample_ticket(env, client, organizer, 1, &owner, 1_000);
    client.check_in(organizer, &ticket_id);
    (owner, ticket_id)
}

fn set_time_after_retention(env: &Env) {
    env.ledger()
        .set_timestamp(STARTS_AT + USED_TICKET_RETENTION_SECONDS);
}

#[test]
fn burn_removes_a_used_ticket_once_retention_has_elapsed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (_owner, ticket_id) = used_ticket(&env, &client, &organizer);
    set_time_after_retention(&env);

    client.burn_used_ticket(&organizer, &ticket_id);

    assert_eq!(
        client.try_get_ticket(&ticket_id),
        Err(Ok(Error::TicketNotFound))
    );
}

#[test]
fn burn_is_allowed_at_the_exact_retention_boundary() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (_owner, ticket_id) = used_ticket(&env, &client, &organizer);
    env.ledger()
        .set_timestamp(STARTS_AT + USED_TICKET_RETENTION_SECONDS - 1);
    assert_eq!(
        client.try_burn_used_ticket(&organizer, &ticket_id),
        Err(Ok(Error::RetentionPeriodNotElapsed))
    );

    set_time_after_retention(&env);
    client.burn_used_ticket(&organizer, &ticket_id);

    assert_eq!(
        client.try_get_ticket(&ticket_id),
        Err(Ok(Error::TicketNotFound))
    );
}

#[test]
fn burn_is_rejected_before_retention_elapses() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (owner, ticket_id) = used_ticket(&env, &client, &organizer);

    let result = client.try_burn_used_ticket(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::RetentionPeriodNotElapsed)));
    assert_eq!(client.get_ticket(&ticket_id).owner, owner);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn burn_rejects_a_ticket_that_was_never_checked_in() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    set_time_after_retention(&env);

    let result = client.try_burn_used_ticket(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::TicketNotUsed)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn burn_rejects_a_revoked_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.revoke_ticket(&organizer, &ticket_id);
    set_time_after_retention(&env);

    let result = client.try_burn_used_ticket(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::TicketNotUsed)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Revoked);
}

#[test]
fn burn_is_organizer_only() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (_owner, ticket_id) = used_ticket(&env, &client, &organizer);
    set_time_after_retention(&env);
    let stranger = Address::generate(&env);

    let result = client.try_burn_used_ticket(&stranger, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotOrganizer)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn burn_reports_not_found_for_an_unknown_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    set_time_after_retention(&env);

    let result = client.try_burn_used_ticket(&organizer, &999);
    assert_eq!(result, Err(Ok(Error::TicketNotFound)));
}

#[test]
fn burned_ticket_cannot_be_burned_again() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (_owner, ticket_id) = used_ticket(&env, &client, &organizer);
    set_time_after_retention(&env);
    client.burn_used_ticket(&organizer, &ticket_id);

    let result = client.try_burn_used_ticket(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::TicketNotFound)));
}

#[test]
fn burn_removes_the_metadata_uri_with_the_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket_with_metadata(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
        &Some(String::from_str(&env, "ipfs://ticket")),
    );
    client.check_in(&organizer, &ticket_id);
    set_time_after_retention(&env);

    client.burn_used_ticket(&organizer, &ticket_id);

    assert_eq!(
        client.try_token_uri(&ticket_id),
        Err(Ok(Error::TicketNotFound))
    );
}

#[test]
fn burn_leaves_issuance_counters_unchanged() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (_owner, ticket_id) = used_ticket(&env, &client, &organizer);
    set_time_after_retention(&env);

    client.burn_used_ticket(&organizer, &ticket_id);

    assert_eq!(client.get_event(&1).tickets_issued, 1);
    assert_eq!(client.total_tickets(), 1);
}

#[test]
fn burn_releases_the_owners_balance() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (owner, ticket_id) = used_ticket(&env, &client, &organizer);
    assert_eq!(client.balance(&owner), 1);
    set_time_after_retention(&env);

    client.burn_used_ticket(&organizer, &ticket_id);

    assert_eq!(client.balance(&owner), 0);
}

#[test]
fn burned_ticket_cannot_be_transferred() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (owner, ticket_id) = used_ticket(&env, &client, &organizer);
    set_time_after_retention(&env);
    client.burn_used_ticket(&organizer, &ticket_id);
    let recipient = Address::generate(&env);

    let result = client.try_transfer_ticket(&owner, &ticket_id, &recipient);
    assert_eq!(result, Err(Ok(Error::TicketNotFound)));
}
