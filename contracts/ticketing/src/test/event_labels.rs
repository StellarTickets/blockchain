//! Event label validation for `create_event` (Issue #125).
//!
//! `create_event` accepted empty `name` and `category` strings, so an event
//! could be created with no human-readable identity. Both are now rejected
//! when empty, along with the oversized case handled in
//! [`crate::test::ticket_labels`].

use super::*;

#[test]
fn create_event_rejects_empty_name() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, ""),
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
    assert_eq!(result, Err(Ok(Error::EmptyNameOrCategory)));
}

#[test]
fn create_event_rejects_empty_category() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Radiohead Live"),
            category: String::from_str(&env, ""),
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
    assert_eq!(result, Err(Ok(Error::EmptyNameOrCategory)));
}

#[test]
fn create_event_rejects_whitespace_free_single_char_boundary() {
    // One byte is the smallest accepted label, MAX_* is the largest.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    client.create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "A"),
            category: String::from_str(&env, "c"),
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
    let event = client.get_event(&1);
    assert_eq!(event.name.len(), 1);
    assert_eq!(event.category.len(), 1);
}

#[test]
fn create_event_accepts_labels_at_the_maximum_length() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let name = "n".repeat(MAX_NAME_LEN as usize);
    let category = "c".repeat(MAX_CATEGORY_LEN as usize);

    client.create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, &name),
            category: String::from_str(&env, &category),
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
    let event = client.get_event(&1);
    assert_eq!(event.name.len(), MAX_NAME_LEN);
    assert_eq!(event.category.len(), MAX_CATEGORY_LEN);
}

#[test]
fn create_event_rejects_oversized_name() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let name = "n".repeat(MAX_NAME_LEN as usize + 1);
    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, &name),
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
    assert_eq!(result, Err(Ok(Error::StringTooLong)));
}

#[test]
fn create_event_rejects_oversized_category() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let category = "c".repeat(MAX_CATEGORY_LEN as usize + 1);
    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Radiohead Live"),
            category: String::from_str(&env, &category),
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
    assert_eq!(result, Err(Ok(Error::StringTooLong)));
}

#[test]
fn create_event_with_options_rejects_empty_name() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_create_event_with_options(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, ""),
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
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::EmptyNameOrCategory)));
}

#[test]
fn rejected_event_is_not_persisted() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let _ = client.try_create_event(
        &organizer,
        &7,
        &EventParams {
            name: String::from_str(&env, ""),
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

    assert_eq!(client.try_get_event(&7), Err(Ok(Error::EventNotFound)));
}
