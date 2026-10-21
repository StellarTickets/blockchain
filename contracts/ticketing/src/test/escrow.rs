//! Escrow on primary sales, together with a per-event payment token
//! (issue #331).
//!
//! Escrow and per-event payment tokens are covered on their own elsewhere,
//! but they also have to work together. When an event names its own payment
//! token and opts into escrow, the escrow balance must be held in the event's
//! own token — not in the contract-wide one — so the buyer, the admin and the
//! organizer all settle in the token the event accepts.

use super::*;

/// Registers a second token contract, returning its address and clients.
///
/// The contract-wide token is the one the contract was initialized with, so a
/// per-event token has to be a different contract for the two balances to be
/// observable separately.
fn register_event_token(env: &Env) -> (Address, TokenClient<'_>, StellarAssetClient<'_>) {
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = TokenClient::new(env, &token_contract.address());
    let token_asset = StellarAssetClient::new(env, &token_contract.address());
    (token_contract.address(), token, token_asset)
}

/// A primary sale on an event that uses both escrow and a per-event payment
/// token must leave the escrow in the event's own token.
///
/// The balance assertion is the point of the test: the contract holds exactly
/// the sale proceeds in the event token, and the contract-wide token is not
/// touched at all.
#[test]
fn purchase_primary_holds_escrow_in_the_per_event_payment_token() {
    let (env, client, token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let (event_token_address, event_token, event_token_asset) = register_event_token(&env);
    client.set_event_payment_token(&organizer, &1, &Some(event_token_address.clone()));
    client.enable_escrow(&organizer, &1, &500u32);
    assert_eq!(client.event_payment_token(&1), event_token_address);

    let price = 2_000i128;
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &price);

    let buyer = Address::generate(&env);
    event_token_asset.mint(&buyer, &10_000i128);

    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    // The escrow is held in the event's own token: the contract holds exactly
    // the sale proceeds there.
    assert_eq!(event_token.balance(&client.address), price);
    assert_eq!(client.get_event(&1).escrow_balance, price);

    // The contract-wide token is not involved in the sale at all.
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(token.balance(&organizer), 0);

    // The buyer paid in the event token and nothing has been released yet.
    assert_eq!(event_token.balance(&buyer), 10_000 - price);
    assert_eq!(event_token.balance(&organizer), 0);
}

/// The platform fee is paid straight to the admin, so escrow holds only the
/// organizer's share — and both the fee and the escrow are in the event token
/// when the event names one. Releasing the escrow empties the contract's
/// event-token balance and pays the organizer in that same token.
#[test]
fn escrowed_sale_in_a_per_event_token_splits_the_fee_and_releases_in_that_token() {
    let (env, client, token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let (event_token_address, event_token, event_token_asset) = register_event_token(&env);
    client.set_event_payment_token(&organizer, &1, &Some(event_token_address));
    client.enable_escrow(&organizer, &1, &500u32);
    client.set_platform_fee_bps(&admin, &250); // 2.5%

    let price = 2_000i128;
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &price);

    let buyer = Address::generate(&env);
    event_token_asset.mint(&buyer, &10_000i128);

    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    // 2.5% of 2_000 is 50, paid to the admin in the event token.
    let fee = 50i128;
    assert_eq!(event_token.balance(&admin), fee);
    // Escrow holds only the organizer's share, also in the event token.
    assert_eq!(event_token.balance(&client.address), price - fee);
    assert_eq!(client.get_event(&1).escrow_balance, price - fee);
    assert_eq!(event_token.balance(&buyer), 10_000 - price);
    // Nothing landed in the contract-wide token.
    assert_eq!(token.balance(&admin), 0);
    assert_eq!(token.balance(&client.address), 0);

    env.ledger().set_sequence_number(500);
    client.release_escrow(&organizer, &1);

    // The release pays out in the event token too, leaving nothing behind.
    assert_eq!(event_token.balance(&organizer), price - fee);
    assert_eq!(event_token.balance(&client.address), 0);
    assert_eq!(token.balance(&organizer), 0);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.get_event(&1).escrow_balance, 0);
}
