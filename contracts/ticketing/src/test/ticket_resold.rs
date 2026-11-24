//! `TicketResold` event on `buy_resale` (issue #117).
//!
//! Indexers need resale sales with the price and royalty split, so they can
//! report secondary-market volume without re-deriving the fee. These tests
//! compare the exact XDR event the contract publishes.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

#[test]
fn buy_resale_publishes_ticket_resold_with_price_and_royalty() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 5% royalty
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_100);
    token_asset.mint(&buyer, &10_000i128);

    client.buy_resale(&buyer, &ticket_id);

    // 5% of 1100 = 55 royalty to the organizer.
    let expected = TicketResold {
        ticket_id,
        seller: seller.clone(),
        buyer: buyer.clone(),
        price: 1_100,
        royalty: 55,
        platform_fee: 0,
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
fn rejected_resale_purchase_publishes_no_ticket_resold() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    // Never listed, so the purchase is rejected before any payment moves.
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    token_asset.mint(&buyer, &10_000i128);

    let result = client.try_buy_resale(&buyer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotForResale)));

    let expected = TicketResold {
        ticket_id,
        seller: seller.clone(),
        buyer: buyer.clone(),
        price: 1_100,
        royalty: 55,
        platform_fee: 0,
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

#[test]
fn buy_resale_publishes_the_platform_fee_and_seller_net() {
    let (env, client, token, token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 5% royalty
    let recipient = Address::generate(&env);
    // 2% resale platform fee (issue #503).
    client.set_platform_fee(&admin, &0, &200, &recipient);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_100);
    token_asset.mint(&buyer, &10_000i128);
    let seller_before = token.balance(&seller);

    client.buy_resale(&buyer, &ticket_id);

    // 2% of 1100 = 22 platform fee, 5% of 1100 = 55 royalty.
    let expected = TicketResold {
        ticket_id,
        seller: seller.clone(),
        buyer: buyer.clone(),
        price: 1_100,
        royalty: 55,
        platform_fee: 22,
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
    // Seller nets price - royalty - platform_fee = 1023.
    assert_eq!(token.balance(&seller) - seller_before, 1_023);
}
