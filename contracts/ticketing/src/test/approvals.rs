//! Per-ticket operator approvals and `transfer_from` (issue #107).
//!
//! A marketplace operator approved for one ticket can move that ticket on the
//! owner's behalf. Any change of owner clears the approval.

use super::*;

/// Issues one ticket on event 1 to `owner` and returns its id.
fn owned_ticket(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    owner: &Address,
) -> u64 {
    issue_sample_ticket(env, client, organizer, 1, owner, 1_000)
}

#[test]
fn approved_operator_moves_the_ticket_and_the_approval_ends() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);

    client.approve(&owner, &ticket_id, &operator);
    assert_eq!(client.get_approved(&ticket_id), Some(operator.clone()));

    client.transfer_from(&operator, &owner, &recipient, &ticket_id);

    assert_eq!(client.owner_of(&ticket_id), recipient);
    assert_eq!(client.get_approved(&ticket_id), None);
}

#[test]
fn only_the_approved_operator_can_transfer_from() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let stranger = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);
    client.approve(&owner, &ticket_id, &operator);

    assert_eq!(
        client.try_transfer_from(&stranger, &owner, &recipient, &ticket_id),
        Err(Ok(Error::NotApproved))
    );
    assert_eq!(client.owner_of(&ticket_id), owner);
}

#[test]
fn a_direct_transfer_clears_the_operator_approval() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let next_owner = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);
    client.approve(&owner, &ticket_id, &operator);

    client.transfer_ticket(&owner, &ticket_id, &recipient);

    // The old operator must not carry over to the new holder.
    assert_eq!(client.get_approved(&ticket_id), None);
    assert_eq!(
        client.try_transfer_from(&operator, &recipient, &next_owner, &ticket_id),
        Err(Ok(Error::NotApproved))
    );
    assert_eq!(client.owner_of(&ticket_id), recipient);
}

#[test]
fn owner_can_revoke_an_approval() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);
    client.approve(&owner, &ticket_id, &operator);

    client.revoke_approval(&owner, &ticket_id);

    assert_eq!(client.get_approved(&ticket_id), None);
    assert_eq!(
        client.try_transfer_from(&operator, &owner, &recipient, &ticket_id),
        Err(Ok(Error::NotApproved))
    );
}

#[test]
fn only_the_owner_can_approve_an_operator() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);
    let operator = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);

    assert_eq!(
        client.try_approve(&stranger, &ticket_id, &operator),
        Err(Ok(Error::NotOwner))
    );
    assert_eq!(client.get_approved(&ticket_id), None);
}

#[test]
fn owner_cannot_approve_themselves() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);

    assert_eq!(
        client.try_approve(&owner, &ticket_id, &owner),
        Err(Ok(Error::SelfTransfer))
    );
}

#[test]
fn transfer_from_checks_the_current_owner() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let stranger = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);
    client.approve(&owner, &ticket_id, &operator);

    // The operator is approved, but the named `from` does not own the ticket.
    assert_eq!(
        client.try_transfer_from(&operator, &stranger, &recipient, &ticket_id),
        Err(Ok(Error::NotOwner))
    );
    assert_eq!(client.owner_of(&ticket_id), owner);
}

#[test]
fn transfer_from_respects_soulbound_events() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_transferable(&organizer, &1, &false);
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = owned_ticket(&env, &client, &organizer, &owner);
    client.approve(&owner, &ticket_id, &operator);

    assert_eq!(
        client.try_transfer_from(&operator, &owner, &recipient, &ticket_id),
        Err(Ok(Error::NotTransferable))
    );
}
