//! Self-transfer and self-purchase guards (Issue #128).
//!
//! `transfer_ticket` accepted `from == to` and `buy_resale` let a seller buy
//! their own listing. Both burn a transfer slot against the per-ticket limit
//! and discard the resale price / gift claim without moving ownership.

use super::*;

fn listed_ticket(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
) -> (Address, u64) {
    make_event(env, client, organizer, 1);
    let seller = Address::generate(env);
    let ticket_id = client.issue_ticket(
        organizer,
        &1,
        &seller,
        &String::from_str(env, "GA"),
        &String::from_str(env, "A12"),
        &1_000i128,
    );
    client.list_for_resale(&seller, &ticket_id, &1_200i128);
    (seller, ticket_id)
}

#[test]
fn transfer_ticket_rejects_self_transfer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A12"),
        &1_000i128,
    );

    let result = client.try_transfer_ticket(&owner, &ticket_id, &owner);
    assert_eq!(result, Err(Ok(Error::SelfTransfer)));
}

#[test]
fn self_transfer_leaves_the_ticket_untouched() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A12"),
        &1_000i128,
    );

    let _ = client.try_transfer_ticket(&owner, &ticket_id, &owner);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, owner);
    // The transfer slot was not consumed by a no-op transfer.
    assert_eq!(ticket.transfers, 0);
    assert_eq!(ticket.status, TicketStatus::Valid);
}

#[test]
fn transfer_ticket_still_allows_a_real_transfer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A12"),
        &1_000i128,
    );

    client.transfer_ticket(&owner, &ticket_id, &recipient);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, recipient);
    assert_eq!(ticket.transfers, 1);
}

#[test]
fn buy_resale_rejects_seller_buying_own_listing() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (seller, ticket_id) = listed_ticket(&env, &client, &organizer);

    let result = client.try_buy_resale(&seller, &ticket_id);
    assert_eq!(result, Err(Ok(Error::SelfPurchase)));
}

#[test]
fn self_purchase_leaves_the_listing_intact() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let (seller, ticket_id) = listed_ticket(&env, &client, &organizer);

    let _ = client.try_buy_resale(&seller, &ticket_id);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, seller);
    assert_eq!(ticket.status, TicketStatus::Resale);
    assert_eq!(ticket.resale_price, 1_200);
    // The transfer slot was not consumed.
    assert_eq!(ticket.transfers, 0);
}

#[test]
fn buy_resale_still_allows_a_third_party_purchase() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    let (_seller, ticket_id) = listed_ticket(&env, &client, &organizer);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 1_200);

    client.buy_resale(&buyer, &ticket_id);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
    assert_eq!(ticket.transfers, 1);
}

#[test]
fn transfer_batch_rejects_self_transfer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A12"),
        &1_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(ticket_id);

    let result = client.try_transfer_batch(&owner, &batch, &owner);
    assert_eq!(result, Err(Ok(Error::SelfTransfer)));
}

#[test]
fn transfer_batch_self_transfer_leaves_tickets_untouched() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A12"),
        &1_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(ticket_id);

    let _ = client.try_transfer_batch(&owner, &batch, &owner);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, owner);
    assert_eq!(ticket.transfers, 0);
    assert_eq!(ticket.status, TicketStatus::Valid);
}

#[test]
fn transfer_batch_still_allows_real_transfer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A12"),
        &1_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(ticket_id);

    client.transfer_batch(&owner, &batch, &recipient);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, recipient);
    assert_eq!(ticket.transfers, 1);
}
