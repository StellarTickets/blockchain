//! Multi-recipient royalty splits (issue #90).
//!
//! `set_royalty_splits` divides the event's `royalty_bps` between several
//! recipients. `buy_resale` pays each recipient their share of the royalty.

use super::*;

/// Lists a ticket issued at 1_000 for 1_100 and returns its id, so the royalty
/// on the resale is 5% of 1_100 = 55.
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

#[test]
fn two_recipient_split_pays_each_share_of_the_royalty() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 500 bps royalty
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let venue = Address::generate(&env);
    let promoter = Address::generate(&env);
    client.set_royalty_splits(
        &organizer,
        &1,
        &Some(soroban_sdk::vec![
            &env,
            (venue.clone(), 300u32),
            (promoter.clone(), 200u32),
        ]),
    );
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    // 55 royalty: venue 300/500 = 33, promoter takes the remaining 22.
    assert_eq!(token.balance(&venue), 33);
    assert_eq!(token.balance(&promoter), 22);
    assert_eq!(token.balance(&organizer), 0);
    assert_eq!(token.balance(&seller), 1_045);
    assert_eq!(client.get_ticket(&ticket_id).owner, buyer);
}

#[test]
fn three_recipient_split_pays_each_share_and_the_remainder_to_the_last() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 500 bps royalty
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let venue = Address::generate(&env);
    let promoter = Address::generate(&env);
    let artist = Address::generate(&env);
    client.set_royalty_splits(
        &organizer,
        &1,
        &Some(soroban_sdk::vec![
            &env,
            (venue.clone(), 200u32),
            (promoter.clone(), 200u32),
            (artist.clone(), 100u32),
        ]),
    );
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    // 55 royalty: venue 22, promoter 22, artist takes the remaining 11.
    assert_eq!(token.balance(&venue), 22);
    assert_eq!(token.balance(&promoter), 22);
    assert_eq!(token.balance(&artist), 11);
    assert_eq!(token.balance(&seller), 1_045);
}

#[test]
fn split_entries_must_sum_to_the_royalty_bps() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 500 bps royalty
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    assert_eq!(
        client.try_set_royalty_splits(
            &organizer,
            &1,
            &Some(soroban_sdk::vec![
                &env,
                (a.clone(), 300u32),
                (b.clone(), 100u32)
            ]),
        ),
        Err(Ok(Error::InvalidRoyalty))
    );
    assert_eq!(
        client.try_set_royalty_splits(
            &organizer,
            &1,
            &Some(soroban_sdk::vec![&env, (a, 300u32), (b, 300u32)]),
        ),
        Err(Ok(Error::InvalidRoyalty))
    );
}

#[test]
fn an_empty_split_list_is_rejected() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let empty: Vec<(Address, u32)> = Vec::new(&env);

    assert_eq!(
        client.try_set_royalty_splits(&organizer, &1, &Some(empty)),
        Err(Ok(Error::InvalidRoyalty))
    );
}

#[test]
fn clearing_the_splits_pays_the_organizer_again() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 500 bps royalty
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let venue = Address::generate(&env);
    client.set_royalty_splits(
        &organizer,
        &1,
        &Some(soroban_sdk::vec![&env, (venue.clone(), 500u32)]),
    );
    client.set_royalty_splits(&organizer, &1, &None);
    let ticket_id = list_sample_ticket(&env, &client, &organizer, &seller);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(token.balance(&organizer), 55);
    assert_eq!(token.balance(&venue), 0);
}

#[test]
fn only_the_organizer_can_set_royalty_splits() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);
    let a = Address::generate(&env);

    assert_eq!(
        client.try_set_royalty_splits(&stranger, &1, &Some(soroban_sdk::vec![&env, (a, 500u32)]),),
        Err(Ok(Error::NotOrganizer))
    );
}
