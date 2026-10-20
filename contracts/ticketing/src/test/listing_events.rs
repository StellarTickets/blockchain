//! `TicketListed` and `ListingCancelled` events on `list_for_resale` and
//! `cancel_resale` (issue #116).
//!
//! Indexers need listings pushed to them so they do not have to poll
//! `get_ticket` for every ticket. These tests compare the exact XDR events the
//! contract publishes.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

#[test]
fn list_for_resale_publishes_ticket_listed_with_owner_and_price() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.list_for_resale(&owner, &ticket_id, &1_100);

    let expected = TicketListed {
        ticket_id,
        owner: owner.clone(),
        price: 1_100,
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
fn rejected_listing_publishes_no_ticket_listed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // cap is 120% of face value
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let result = client.try_list_for_resale(&owner, &ticket_id, &1_201);
    assert_eq!(result, Err(Ok(Error::ResalePriceExceedsCap)));

    let expected = TicketListed {
        ticket_id,
        owner: owner.clone(),
        price: 1_201,
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
fn cancel_resale_publishes_listing_cancelled_with_ticket_and_owner() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100);

    client.cancel_resale(&owner, &ticket_id);

    let expected = ListingCancelled {
        ticket_id,
        owner: owner.clone(),
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
fn rejected_cancel_publishes_no_listing_cancelled() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    // Never listed, so there is no listing to cancel.
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let result = client.try_cancel_resale(&owner, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotForResale)));

    let expected = ListingCancelled {
        ticket_id,
        owner: owner.clone(),
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
