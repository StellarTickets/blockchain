//! `TicketTransferred` event on direct transfers (issue #115).
//!
//! A direct transfer changes a ticket's owner, so indexers need an event that
//! carries both sides of the move. These tests compare the exact XDR event the
//! contract publishes.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

#[test]
fn transfer_ticket_publishes_ticket_transferred_with_from_and_to() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.transfer_ticket(&owner, &ticket_id, &recipient);

    let expected = TicketTransferred {
        ticket_id,
        from: owner.clone(),
        to: recipient.clone(),
    }
    .to_xdr(&env, &client.address);
    let published = env
        .events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count();
    assert_eq!(published, 1);
}

#[test]
fn rejected_transfer_publishes_no_ticket_transferred_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let result = client.try_transfer_ticket(&stranger, &ticket_id, &recipient);
    assert_eq!(result, Err(Ok(Error::NotOwner)));

    let expected = TicketTransferred {
        ticket_id,
        from: stranger.clone(),
        to: recipient.clone(),
    }
    .to_xdr(&env, &client.address);
    let published = env
        .events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count();
    assert_eq!(published, 0);
}

/// Counts how many `TicketTransferred` events for one exact move the last
/// contract call published (issue #513).
fn count_ticket_transferred(
    env: &Env,
    client: &TicketingContractClient,
    ticket_id: u64,
    from: &Address,
    to: &Address,
) -> usize {
    let expected = TicketTransferred {
        ticket_id,
        from: from.clone(),
        to: to.clone(),
    }
    .to_xdr(env, &client.address);
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count()
}

#[test]
fn buy_resale_publishes_ticket_transferred_from_seller_to_buyer() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_100);
    token_asset.mint(&buyer, &10_000i128);

    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(
        count_ticket_transferred(&env, &client, ticket_id, &seller, &buyer),
        1
    );
}

#[test]
fn accept_offer_publishes_ticket_transferred_from_seller_to_buyer() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &900i128);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_000);
    client.make_offer(&buyer, &ticket_id, &900);

    client.accept_offer(&seller, &ticket_id, &buyer);

    assert_eq!(
        count_ticket_transferred(&env, &client, ticket_id, &seller, &buyer),
        1
    );
}

#[test]
fn settle_auction_publishes_ticket_transferred_to_the_winner() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let winner = Address::generate(&env);
    token_asset.mint(&winner, &1_000i128);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.start_auction(&seller, &ticket_id, &500, &5_000);
    client.place_bid(&winner, &ticket_id, &800);

    env.ledger().set_timestamp(5_000);
    client.settle_auction(&ticket_id);

    assert_eq!(
        count_ticket_transferred(&env, &client, ticket_id, &seller, &winner),
        1
    );
}

#[test]
fn claim_gift_publishes_ticket_transferred_from_gifter_to_recipient() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let secret = Bytes::from_array(&env, &[7u8; 32]);
    let secret_hash = env.crypto().sha256(&secret).to_bytes();
    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64);

    client.claim_gift(&recipient, &ticket_id, &secret);

    assert_eq!(
        count_ticket_transferred(&env, &client, ticket_id, &owner, &recipient),
        1
    );
}

#[test]
fn transfer_batch_publishes_one_ticket_transferred_per_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let first = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let second = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.transfer_batch(&owner, &soroban_sdk::vec![&env, first, second], &recipient);

    assert_eq!(
        count_ticket_transferred(&env, &client, first, &owner, &recipient),
        1
    );
    assert_eq!(
        count_ticket_transferred(&env, &client, second, &owner, &recipient),
        1
    );
}
