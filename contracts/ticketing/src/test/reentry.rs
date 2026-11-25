//! Re-entry pass-outs (issue #78).
//!
//! `pass_out` returns a checked-in ticket to `Valid` so it can be checked in
//! again. The event's `max_reentries` limits how many times a ticket may do so,
//! and zero turns re-entry off.

use super::*;

/// Issues a ticket on an existing event and checks it in.
fn issue_and_check_in(env: &Env, client: &TicketingContractClient, organizer: &Address) -> u64 {
    let ticket_id = issue_sample_ticket(env, client, organizer, 1, &Address::generate(env), 1_000);
    client.check_in(organizer, &ticket_id);
    ticket_id
}

#[test]
fn pass_out_returns_a_checked_in_ticket_to_valid_for_re_entry() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_max_reentries(&organizer, &1, &2u32);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);

    assert_eq!(client.pass_out(&organizer, &ticket_id), 1);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    assert_eq!(client.reentry_count(&ticket_id), 1);

    client.check_in(&organizer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn pass_out_rejects_a_ticket_that_is_not_checked_in() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_max_reentries(&organizer, &1, &1u32);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_eq!(
        client.try_pass_out(&organizer, &ticket_id),
        Err(Ok(Error::TicketNotUsed))
    );
}

#[test]
fn pass_out_is_rejected_when_the_event_disables_re_entry() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);

    assert_eq!(
        client.try_pass_out(&organizer, &ticket_id),
        Err(Ok(Error::ReentryNotAllowed))
    );
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn pass_out_stops_at_the_re_entry_limit() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_max_reentries(&organizer, &1, &1u32);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);

    assert_eq!(client.pass_out(&organizer, &ticket_id), 1);
    client.check_in(&organizer, &ticket_id);

    assert_eq!(
        client.try_pass_out(&organizer, &ticket_id),
        Err(Ok(Error::ReentryLimitReached))
    );
    assert_eq!(client.reentry_count(&ticket_id), 1);
}

#[test]
fn only_the_organizer_can_pass_a_ticket_out() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_max_reentries(&organizer, &1, &1u32);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_pass_out(&stranger, &ticket_id),
        Err(Ok(Error::NotOrganizer))
    );
}

// Issue #490: Check-in signers can pass tickets out for re-entry.

#[test]
fn check_in_signer_can_pass_out_a_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_max_reentries(&organizer, &1, &2u32);
    let gate = Address::generate(&env);
    client.add_checkin_signer(&organizer, &1, &gate);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);

    // Signer can pass out
    assert_eq!(client.pass_out(&gate, &ticket_id), 1);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    assert_eq!(client.reentry_count(&ticket_id), 1);

    // And check back in
    client.check_in(&gate, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

#[test]
fn removed_signer_cannot_pass_out() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_max_reentries(&organizer, &1, &1u32);
    let gate = Address::generate(&env);
    client.add_checkin_signer(&organizer, &1, &gate);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);

    // Remove the signer
    client.remove_checkin_signer(&organizer, &1, &gate);

    // Now they cannot pass out
    assert_eq!(
        client.try_pass_out(&gate, &ticket_id),
        Err(Ok(Error::NotOrganizer))
    );
}

#[test]
fn signer_from_different_event_cannot_pass_out() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &organizer, 2);
    client.set_event_max_reentries(&organizer, &1, &1u32);
    let gate = Address::generate(&env);
    // Gate is a signer for event 2, not event 1
    client.add_checkin_signer(&organizer, &2, &gate);
    let ticket_id = issue_and_check_in(&env, &client, &organizer);

    assert_eq!(
        client.try_pass_out(&gate, &ticket_id),
        Err(Ok(Error::NotOrganizer))
    );
}
