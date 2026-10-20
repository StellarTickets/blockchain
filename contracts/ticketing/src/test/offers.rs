//! Escrowed offers on listed tickets (issue #106).
//!
//! An offer is escrowed when it is made. The owner it was made to can accept
//! it, which settles it like a resale purchase, or reject it. The buyer can
//! withdraw it at any time.

use super::*;

/// Lists a ticket for 1_000, its face value, and returns its id.
fn listed_ticket(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    seller: &Address,
) -> u64 {
    let ticket_id = issue_sample_ticket(env, client, organizer, 1, seller, 1_000);
    client.list_for_resale(seller, &ticket_id, &1_000);
    ticket_id
}

#[test]
fn offer_is_escrowed_and_returned_when_the_buyer_withdraws_it() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 900);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);

    client.make_offer(&buyer, &ticket_id, &900);
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.balance(&client.address), 900);
    let offer = client.get_offer(&ticket_id, &buyer);
    assert_eq!(offer.amount, 900);
    assert_eq!(offer.seller, seller);

    client.cancel_offer(&buyer, &ticket_id, &buyer);
    assert_eq!(token.balance(&buyer), 900);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(
        client.try_get_offer(&ticket_id, &buyer),
        Err(Ok(Error::OfferNotFound))
    );
}

#[test]
fn accepted_offer_transfers_the_ticket_and_settles_the_payments() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 900);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);

    client.make_offer(&buyer, &ticket_id, &900);
    client.accept_offer(&seller, &ticket_id, &buyer);

    assert_eq!(client.owner_of(&ticket_id), buyer);
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
    // The 5% royalty on the 900 offer goes to the organizer, the rest to the seller.
    assert_eq!(token.balance(&organizer), 45);
    assert_eq!(token.balance(&seller), 855);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(
        client.try_get_offer(&ticket_id, &buyer),
        Err(Ok(Error::OfferNotFound))
    );
}

#[test]
fn offers_are_capped_at_the_resale_multiplier() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // cap is 120% of face value
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 2_000);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_make_offer(&buyer, &ticket_id, &1_201),
        Err(Ok(Error::ResalePriceExceedsCap))
    );
    client.make_offer(&buyer, &ticket_id, &1_200);
}

#[test]
fn offer_on_a_ticket_that_is_not_listed_is_rejected() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 900);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);

    assert_eq!(
        client.try_make_offer(&buyer, &ticket_id, &900),
        Err(Ok(Error::NotForResale))
    );
}

#[test]
fn buyer_cannot_hold_two_open_offers_on_one_ticket() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 1_800);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);

    client.make_offer(&buyer, &ticket_id, &900);
    assert_eq!(
        client.try_make_offer(&buyer, &ticket_id, &950),
        Err(Ok(Error::OfferAlreadyExists))
    );
    // The rejected second offer took no funds.
    assert_eq!(token.balance(&client.address), 900);
}

#[test]
fn owner_can_reject_an_offer_and_the_buyer_is_refunded() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 900);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);
    client.make_offer(&buyer, &ticket_id, &900);

    client.cancel_offer(&seller, &ticket_id, &buyer);

    assert_eq!(token.balance(&buyer), 900);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.owner_of(&ticket_id), seller);
}

#[test]
fn a_stranger_cannot_accept_or_cancel_someone_elses_offer() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let stranger = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 900);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);
    client.make_offer(&buyer, &ticket_id, &900);

    assert_eq!(
        client.try_cancel_offer(&stranger, &ticket_id, &buyer),
        Err(Ok(Error::NotOwner))
    );
    assert_eq!(
        client.try_accept_offer(&stranger, &ticket_id, &buyer),
        Err(Ok(Error::NotOwner))
    );
    assert_eq!(token.balance(&client.address), 900);
}

#[test]
fn accepting_a_missing_offer_is_rejected() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_accept_offer(&seller, &ticket_id, &buyer),
        Err(Ok(Error::OfferNotFound))
    );
}

#[test]
fn owner_cannot_make_an_offer_on_their_own_listing() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    mint_tokens(&token_asset, &seller, 900);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);

    assert_eq!(
        client.try_make_offer(&seller, &ticket_id, &900),
        Err(Ok(Error::SelfPurchase))
    );
}

#[test]
fn offer_cannot_be_accepted_after_the_ticket_changes_hands() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let third = Address::generate(&env);
    mint_tokens(&token_asset, &buyer, 900);
    let ticket_id = listed_ticket(&env, &client, &organizer, &seller);
    client.make_offer(&buyer, &ticket_id, &900);

    // A direct transfer ends the seller's ownership, and with it the offer.
    client.transfer_ticket(&seller, &ticket_id, &third);

    assert_eq!(
        client.try_accept_offer(&third, &ticket_id, &buyer),
        Err(Ok(Error::NotOwner))
    );
    assert_eq!(
        client.try_accept_offer(&seller, &ticket_id, &buyer),
        Err(Ok(Error::NotOwner))
    );
    // The buyer can still withdraw the stale offer and recover the funds.
    client.cancel_offer(&buyer, &ticket_id, &buyer);
    assert_eq!(token.balance(&buyer), 900);
    assert_eq!(token.balance(&client.address), 0);
}
