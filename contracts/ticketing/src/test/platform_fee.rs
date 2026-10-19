//! Platform fee on resales (issue #103).
//!
//! The fee comes out of the seller's proceeds, next to the organizer royalty.
//! Fee plus royalty may never exceed the resale price.

use super::*;

/// Issues a ticket at 1_000 and lists it at 1_100. With the 500 bps royalty
/// from `make_event`, the royalty is 55. A 250 bps fee is 27.5, which rounds
/// down to 27, leaving the seller 1_018.
fn list_sample_ticket(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    seller: &Address,
) -> u64 {
    let ticket_id = issue_sample_ticket(env, client, organizer, 1, seller, 1_000);
    client.list_for_resale(seller, &ticket_id, &1_100i128);
    ticket_id
}

fn event_params(env: &Env, royalty_bps: u32) -> EventParams {
    EventParams {
        name: String::from_str(env, "Gala"),
        category: String::from_str(env, "concert"),
        max_resale_multiplier_bps: 12_000,
        royalty_bps,
        starts_at: 10_000,
        transfer_freeze_seconds: 100,
        resale_cutoff_seconds: 200,
    }
}

#[test]
fn fee_is_paid_to_the_recipient_out_of_the_seller_proceeds() {
    let (env, client, token, token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let platform = Address::generate(&env);
    client.set_platform_fee(&admin, &250, &platform);
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(token.balance(&organizer), 55);
    assert_eq!(token.balance(&platform), 27);
    assert_eq!(token.balance(&seller), 1_018);
    assert_eq!(client.get_ticket(&ticket_id).owner, buyer);
}

#[test]
fn quote_resale_reports_the_seller_amount_net_of_the_fee() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let platform = Address::generate(&env);
    client.set_platform_fee(&admin, &250, &platform);
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    // The fee is the difference: 1_100 - 55 - 1_018 = 27.
    assert_eq!(client.quote_resale(&ticket_id), (1_100, 55, 1_018));
}

#[test]
fn no_fee_leaves_resale_settlement_unchanged() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    assert_eq!(client.platform_fee(), None);
    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(token.balance(&organizer), 55);
    assert_eq!(token.balance(&seller), 1_045);
}

#[test]
fn zero_fee_switches_the_fee_off() {
    let (env, client, token, token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let platform = Address::generate(&env);
    client.set_platform_fee(&admin, &250, &platform);
    client.set_platform_fee(&admin, &0, &platform);
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(token.balance(&platform), 0);
    assert_eq!(token.balance(&seller), 1_045);
}

#[test]
fn non_admin_cannot_set_the_fee() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let intruder = Address::generate(&env);
    let platform = Address::generate(&env);

    assert_eq!(
        client.try_set_platform_fee(&intruder, &250, &platform),
        Err(Ok(Error::NotAdmin))
    );
}

#[test]
fn fee_above_the_whole_price_is_rejected() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let platform = Address::generate(&env);

    assert_eq!(
        client.try_set_platform_fee(&admin, &10_001, &platform),
        Err(Ok(Error::InvalidPlatformFee))
    );
    assert_eq!(client.platform_fee(), None);
}

#[test]
fn fee_plus_an_existing_royalty_cannot_exceed_100_percent() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    client.create_event(&organizer, &1, &event_params(&env, 9_000));
    let platform = Address::generate(&env);

    assert_eq!(
        client.try_set_platform_fee(&admin, &1_001, &platform),
        Err(Ok(Error::InvalidPlatformFee))
    );
    client.set_platform_fee(&admin, &1_000, &platform);
    assert_eq!(client.platform_fee().map(|fee| fee.bps), Some(1_000));
}

#[test]
fn new_event_royalty_cannot_exceed_100_percent_with_the_fee() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    let platform = Address::generate(&env);
    client.set_platform_fee(&admin, &1_000, &platform);

    assert_eq!(
        client.try_create_event(&organizer, &1, &event_params(&env, 9_001)),
        Err(Ok(Error::InvalidRoyalty))
    );
    client.create_event(&organizer, &1, &event_params(&env, 9_000));
}
