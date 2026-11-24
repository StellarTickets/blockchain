//! Organizer-initiated delisting (issue #518).
//!
//! `organizer_delist` pulls a resale listing for a compromised or fraudulent
//! ticket and returns the ticket to `Valid` under its owner. It does not
//! revoke the ticket.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

#[test]
fn organizer_delist_returns_a_listed_ticket_to_valid_under_its_owner() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100);

    client.organizer_delist(&organizer, &ticket_id);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
    assert_eq!(client.owner_of(&ticket_id), owner);
}

#[test]
fn delisted_ticket_is_no_longer_cancellable_as_a_listing() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100);
    client.organizer_delist(&organizer, &ticket_id);

    assert_eq!(
        client.try_cancel_resale(&owner, &ticket_id),
        Err(Ok(Error::NotForResale))
    );
    assert_eq!(
        client.try_organizer_delist(&organizer, &ticket_id),
        Err(Ok(Error::NotForResale))
    );
}

#[test]
fn delisting_publishes_listing_delisted() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100);

    client.organizer_delist(&organizer, &ticket_id);

    let expected = ListingDelisted {
        ticket_id,
        organizer: organizer.clone(),
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
fn only_the_event_organizer_can_delist() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100);

    assert_eq!(
        client.try_organizer_delist(&owner, &ticket_id),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Resale);
}

#[test]
fn delisting_an_unlisted_ticket_is_rejected() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_eq!(
        client.try_organizer_delist(&organizer, &ticket_id),
        Err(Ok(Error::NotForResale))
    );
}

#[test]
fn delisting_an_unknown_ticket_is_rejected() {
    let (_env, client, _token, _token_asset, _admin, organizer) = setup();

    assert_eq!(
        client.try_organizer_delist(&organizer, &99),
        Err(Ok(Error::TicketNotFound))
    );
}

#[test]
fn delisting_works_after_the_resale_cutoff() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // starts_at 10_000, cutoff 200s
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    env.ledger().set_timestamp(9_799);
    client.list_for_resale(&owner, &ticket_id, &1_100);

    env.ledger().set_timestamp(9_800);
    client.organizer_delist(&organizer, &ticket_id);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn delisting_works_on_a_cancelled_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100);
    client.cancel_event(&organizer, &1);

    client.organizer_delist(&organizer, &ticket_id);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
}

#[test]
fn auction_with_bids_cannot_be_delisted() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);
    mint_tokens(&token_asset, &bidder, 1_000);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.start_auction(&seller, &ticket_id, &500, &5_000);
    client.place_bid(&bidder, &ticket_id, &600);

    assert_eq!(
        client.try_organizer_delist(&organizer, &ticket_id),
        Err(Ok(Error::AuctionHasBids))
    );
}

#[test]
fn auction_without_bids_is_removed_on_delist() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.start_auction(&seller, &ticket_id, &500, &5_000);

    client.organizer_delist(&organizer, &ticket_id);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    assert_eq!(
        client.try_cancel_resale(&seller, &ticket_id),
        Err(Ok(Error::NotForResale))
    );
}
