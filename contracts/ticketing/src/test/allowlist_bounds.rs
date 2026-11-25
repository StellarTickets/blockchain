//! Tests for allowlist batch size limits (Issue #496).

use super::*;

#[test]
fn add_to_allowlist_accepts_batch_up_to_max_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Create exactly MAX_BATCH_SIZE members
    let mut members = Vec::new(&env);
    for _ in 0..MAX_BATCH_SIZE {
        members.push_back(Address::generate(&env));
    }

    // Should succeed
    client.add_to_allowlist(&organizer, &1, &members);

    // Verify all members were added
    for member in members.iter() {
        assert!(client.is_on_allowlist(&1, &member));
    }
}

#[test]
fn add_to_allowlist_rejects_batch_exceeding_max_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Create MAX_BATCH_SIZE + 1 members
    let mut members = Vec::new(&env);
    for _ in 0..=MAX_BATCH_SIZE {
        members.push_back(Address::generate(&env));
    }

    // Should fail
    assert_eq!(
        client.try_add_to_allowlist(&organizer, &1, &members),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn add_to_allowlist_rejects_empty_list() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let members = Vec::new(&env);

    // Should fail
    assert_eq!(
        client.try_add_to_allowlist(&organizer, &1, &members),
        Err(Ok(Error::EmptyBatch))
    );
}

#[test]
fn remove_from_allowlist_accepts_batch_up_to_max_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Create and add MAX_BATCH_SIZE members
    let mut members = Vec::new(&env);
    for _ in 0..MAX_BATCH_SIZE {
        let member = Address::generate(&env);
        members.push_back(member.clone());
    }
    client.add_to_allowlist(&organizer, &1, &members);

    // Remove them all at once
    client.remove_from_allowlist(&organizer, &1, &members);

    // Verify all members were removed
    for member in members.iter() {
        assert!(!client.is_on_allowlist(&1, &member));
    }
}

#[test]
fn remove_from_allowlist_rejects_batch_exceeding_max_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Create MAX_BATCH_SIZE + 1 members
    let mut members = Vec::new(&env);
    for _ in 0..=MAX_BATCH_SIZE {
        members.push_back(Address::generate(&env));
    }

    // Should fail
    assert_eq!(
        client.try_remove_from_allowlist(&organizer, &1, &members),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn remove_from_allowlist_rejects_empty_list() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let members = Vec::new(&env);

    // Should fail
    assert_eq!(
        client.try_remove_from_allowlist(&organizer, &1, &members),
        Err(Ok(Error::EmptyBatch))
    );
}

#[test]
fn add_to_allowlist_with_single_member_works() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let mut members = Vec::new(&env);
    let member = Address::generate(&env);
    members.push_back(member.clone());

    client.add_to_allowlist(&organizer, &1, &members);

    assert!(client.is_on_allowlist(&1, &member));
}
