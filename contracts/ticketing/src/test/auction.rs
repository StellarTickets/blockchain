//! Timed auctions for resale tickets (issue #105).
//!
//! Bids are escrowed in the contract. Each new high bid refunds the bidder it
//! displaces, and settlement pays the royalty and the seller from the winning
//! bid.

use super::*;
use soroban_sdk::BytesN;

/// Ends the auction before the resale cutoff of `make_event` (starts_at 10_000
/// minus resale_cutoff_seconds 200).
const AUCTION_END: u64 = 5_000;

/// Issues a ticket to `seller` and starts a 500-reserve auction that ends at
/// `AUCTION_END`. Returns the ticket id.
fn auctioned_ticket(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    seller: &Address,
) -> u64 {
    let ticket_id = issue_sample_ticket(env, client, organizer, 1, seller, 1_000);
    client.start_auction(seller, &ticket_id, &500, &AUCTION_END);
    ticket_id
}

#[test]
fn auction_settles_to_the_highest_bidder_and_pays_out_the_winning_bid() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    mint_tokens(&token_asset, &first, 1_000);
    mint_tokens(&token_asset, &second, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    client.place_bid(&first, &ticket_id, &600);
    client.place_bid(&second, &ticket_id, &800);
    // The first bidder was outbid, so the contract refunded them in full.
    assert_eq!(token.balance(&first), 1_000);
    assert_eq!(token.balance(&second), 200);
    assert_eq!(token.balance(&client.address), 800);

    env.ledger().set_timestamp(AUCTION_END);
    client.settle_auction(&ticket_id);

    assert_eq!(client.owner_of(&ticket_id), second);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    // The 5% royalty on the 800 winning bid goes to the organizer, the rest to the seller.
    assert_eq!(token.balance(&organizer), 40);
    assert_eq!(token.balance(&seller), 760);
    assert_eq!(token.balance(&client.address), 0);
}

#[test]
fn a_bid_must_meet_the_reserve_and_beat_the_highest_bid() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    mint_tokens(&token_asset, &first, 1_000);
    mint_tokens(&token_asset, &second, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    // The reserve is 500, so 499 is too low for the opening bid.
    assert_eq!(
        client.try_place_bid(&first, &ticket_id, &499),
        Err(Ok(Error::BidTooLow))
    );
    client.place_bid(&first, &ticket_id, &600);
    // A bid equal to the highest bid does not beat it.
    assert_eq!(
        client.try_place_bid(&second, &ticket_id, &600),
        Err(Ok(Error::BidTooLow))
    );
    client.place_bid(&second, &ticket_id, &601);
}

#[test]
fn bids_above_the_resale_cap_are_rejected() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // cap is 120% of face value
    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);
    mint_tokens(&token_asset, &bidder, 2_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_place_bid(&bidder, &ticket_id, &1_201),
        Err(Ok(Error::ResalePriceExceedsCap))
    );
    client.place_bid(&bidder, &ticket_id, &1_200);
}

#[test]
fn seller_cannot_bid_on_their_own_auction() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    mint_tokens(&token_asset, &seller, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_place_bid(&seller, &ticket_id, &600),
        Err(Ok(Error::SelfPurchase))
    );
}

#[test]
fn settling_before_the_auction_ends_is_rejected() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_settle_auction(&ticket_id),
        Err(Ok(Error::AuctionNotEnded))
    );
}

#[test]
fn bids_are_rejected_once_the_auction_has_ended() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);
    mint_tokens(&token_asset, &bidder, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    env.ledger().set_timestamp(AUCTION_END);
    assert_eq!(
        client.try_place_bid(&bidder, &ticket_id, &600),
        Err(Ok(Error::AuctionEnded))
    );
}

#[test]
fn settling_without_bids_returns_the_ticket_to_valid() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    env.ledger().set_timestamp(AUCTION_END);
    client.settle_auction(&ticket_id);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
    assert_eq!(client.owner_of(&ticket_id), seller);
    assert_eq!(
        client.try_get_auction(&ticket_id),
        Err(Ok(Error::AuctionNotFound))
    );
}

#[test]
fn a_running_auction_blocks_transfers_fixed_price_sales_listings_and_gifts() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);
    let other = Address::generate(&env);
    mint_tokens(&token_asset, &bidder, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_transfer_ticket(&seller, &ticket_id, &other),
        Err(Ok(Error::AuctionActive))
    );
    assert_eq!(
        client.try_list_for_resale(&seller, &ticket_id, &1_000),
        Err(Ok(Error::AuctionActive))
    );
    assert_eq!(
        client.try_create_gift_claim(
            &seller,
            &ticket_id,
            &BytesN::from_array(&env, &[7u8; 32]),
            &9_000,
        ),
        Err(Ok(Error::AuctionActive))
    );
    client.place_bid(&bidder, &ticket_id, &600);
    assert_eq!(
        client.try_buy_resale(&other, &ticket_id),
        Err(Ok(Error::AuctionActive))
    );
    assert_eq!(
        client.try_quote_resale(&ticket_id),
        Err(Ok(Error::AuctionActive))
    );
}

#[test]
fn cancelling_an_auction_without_bids_restores_a_transferable_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let other = Address::generate(&env);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);

    client.cancel_resale(&seller, &ticket_id);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    client.transfer_ticket(&seller, &ticket_id, &other);
    assert_eq!(client.owner_of(&ticket_id), other);
}

#[test]
fn an_auction_with_bids_cannot_be_cancelled() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);
    mint_tokens(&token_asset, &bidder, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);
    client.place_bid(&bidder, &ticket_id, &600);

    assert_eq!(
        client.try_cancel_resale(&seller, &ticket_id),
        Err(Ok(Error::AuctionHasBids))
    );
}

#[test]
fn auction_end_must_be_in_the_future_and_before_resale_closes() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);

    // Resale closes at 10_000 - 200 = 9_800, so an end at 9_900 is too late.
    assert_eq!(
        client.try_start_auction(&seller, &ticket_id, &500, &9_900),
        Err(Ok(Error::InvalidAuctionEnd))
    );
    // The clock is at 0, so an end at 0 is not in the future.
    assert_eq!(
        client.try_start_auction(&seller, &ticket_id, &500, &0),
        Err(Ok(Error::InvalidAuctionEnd))
    );
}

#[test]
fn a_fixed_price_listing_cannot_be_turned_into_an_auction() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_000);

    assert_eq!(
        client.try_start_auction(&seller, &ticket_id, &500, &AUCTION_END),
        Err(Ok(Error::ResaleListingActive))
    );
}

#[test]
fn revoking_a_ticket_mid_auction_refunds_the_highest_bid() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);
    mint_tokens(&token_asset, &bidder, 1_000);
    let ticket_id = auctioned_ticket(&env, &client, &organizer, &seller);
    client.place_bid(&bidder, &ticket_id, &600);
    assert_eq!(token.balance(&bidder), 400);

    client.revoke_ticket(&organizer, &ticket_id);
    env.ledger().set_timestamp(AUCTION_END);
    client.settle_auction(&ticket_id);

    // The ticket stays revoked, so no ownership moves and the bid is returned.
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Revoked);
    assert_eq!(client.owner_of(&ticket_id), seller);
    assert_eq!(token.balance(&bidder), 1_000);
    assert_eq!(token.balance(&client.address), 0);
}
