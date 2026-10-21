//! `EventCreated` event on `create_event` (issue #119).
//!
//! Indexers key events by organizer and event id, so both are topics. These
//! tests compare the exact XDR event the contract publishes.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

#[test]
fn create_event_publishes_event_created_with_organizer_and_event_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    make_event(&env, &client, &organizer, 1);

    let expected = EventCreated {
        organizer: organizer.clone(),
        event_id: 1,
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
fn rejected_create_event_publishes_no_event_created() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let stranger = Address::generate(&env);

    let result = client.try_create_event(
        &stranger,
        &1,
        &EventParams {
            name: String::from_str(&env, "Radiohead Live"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(result, Err(Ok(Error::OrganizerNotApproved)));

    let expected = EventCreated {
        organizer: stranger.clone(),
        event_id: 1,
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
