//! Duplicate ids in a batch (issue #510).
//!
//! A batch that names the same ticket twice used to fail on the second
//! occurrence with a misleading `NotOwner` (or `Revoked`, `AlreadyUsed`).
//! The batch entry points now reject the duplicate up front with
//! `DuplicateTicketInBatch`, before any ticket is read or changed.

use super::*;

#[test]
fn transfer_batch_rejects_a_duplicate_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let result = client.try_transfer_batch(
        &owner,
        &Vec::from_array(&env, [ticket_id, ticket_id]),
        &recipient,
    );

    assert_eq!(result, Err(Ok(Error::DuplicateTicketInBatch)));
    assert_eq!(client.get_ticket(&ticket_id).owner, owner);
}

#[test]
fn check_in_batch_rejects_a_duplicate_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let result =
        client.try_check_in_batch(&organizer, &Vec::from_array(&env, [ticket_id, ticket_id]));

    assert_eq!(result, Err(Ok(Error::DuplicateTicketInBatch)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn revoke_batch_rejects_a_duplicate_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let result =
        client.try_revoke_batch(&organizer, &Vec::from_array(&env, [ticket_id, ticket_id]));

    assert_eq!(result, Err(Ok(Error::DuplicateTicketInBatch)));
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    assert_eq!(client.get_event(&1).tickets_issued, 1);
}

#[test]
fn duplicate_is_reported_before_ownership_is_checked() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    // The stranger does not own the ticket, but the duplicate is the first
    // problem the batch has, so that is the error reported.
    let result = client.try_transfer_batch(
        &stranger,
        &Vec::from_array(&env, [ticket_id, ticket_id]),
        &recipient,
    );

    assert_eq!(result, Err(Ok(Error::DuplicateTicketInBatch)));
}

#[test]
fn distinct_ids_in_a_batch_are_still_accepted() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let first = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let second = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.transfer_batch(&owner, &Vec::from_array(&env, [first, second]), &recipient);

    assert_eq!(client.get_ticket(&first).owner, recipient);
    assert_eq!(client.get_ticket(&second).owner, recipient);
}
