//! Organizer allowlist for event creation (Issue #135).
//!
//! `event_id` is chosen off-chain by the backend, and before this change any
//! address could call `create_event` with an unused id, squatting on the id
//! the backend was about to register. Event creation is now restricted to
//! organizers the admin has approved. See `docs/ORGANIZER_ALLOWLIST.md`.

use super::*;

fn try_create(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    event_id: u64,
) -> Result<Result<(), soroban_sdk::ConversionError>, Result<Error, soroban_sdk::InvokeError>> {
    client.try_create_event(
        organizer,
        &event_id,
        &EventParams {
            name: String::from_str(env, "Squatted Event"),
            category: String::from_str(env, "concert"),
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
    )
}

#[test]
fn unapproved_address_cannot_squat_an_event_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let squatter = Address::generate(&env);

    // The squatter tries to claim the id the backend is about to use.
    let result = try_create(&env, &client, &squatter, 42);
    assert_eq!(result, Err(Ok(Error::OrganizerNotApproved)));
    assert_eq!(client.try_get_event(&42), Err(Ok(Error::EventNotFound)));
    assert_eq!(client.get_organizer_events(&squatter), 0);

    // The id is still free for the legitimate, approved organizer.
    make_event(&env, &client, &organizer, 42);
    assert_eq!(client.get_event(&42).organizer, organizer);
}

#[test]
fn create_event_with_options_also_requires_an_approved_organizer() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let squatter = Address::generate(&env);

    let result = client.try_create_event_with_options(
        &squatter,
        &7,
        &EventParams {
            name: String::from_str(&env, "Squatted Event"),
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
    assert_eq!(result, Err(Ok(Error::OrganizerNotApproved)));
    assert_eq!(client.try_get_event(&7), Err(Ok(Error::EventNotFound)));
}

#[test]
fn admin_can_approve_a_new_organizer() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let newcomer = Address::generate(&env);
    assert!(!client.is_approved_organizer(&newcomer));

    client.approve_organizer(&admin, &newcomer);
    assert!(client.is_approved_organizer(&newcomer));

    // Approving twice is a no-op, not an error.
    client.approve_organizer(&admin, &newcomer);
    assert!(client.is_approved_organizer(&newcomer));

    make_event(&env, &client, &newcomer, 1);
    assert_eq!(client.get_event(&1).organizer, newcomer);
}

#[test]
fn revoked_organizer_cannot_create_new_events_but_keeps_existing_ones() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.revoke_organizer(&admin, &organizer);
    assert!(!client.is_approved_organizer(&organizer));
    assert_eq!(
        try_create(&env, &client, &organizer, 2),
        Err(Ok(Error::OrganizerNotApproved))
    );

    // Existing events keep working: the allowlist gates creation only.
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &buyer, 1_000);
    client.check_in(&organizer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);

    // Re-approval restores event creation.
    client.approve_organizer(&admin, &organizer);
    make_event(&env, &client, &organizer, 2);
}

#[test]
fn only_the_admin_can_manage_the_allowlist() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let outsider = Address::generate(&env);

    assert_eq!(
        client.try_approve_organizer(&organizer, &outsider),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        client.try_approve_organizer(&outsider, &outsider),
        Err(Ok(Error::NotAdmin))
    );
    assert!(!client.is_approved_organizer(&outsider));

    assert_eq!(
        client.try_revoke_organizer(&outsider, &organizer),
        Err(Ok(Error::NotAdmin))
    );
    assert!(client.is_approved_organizer(&organizer));
}

#[test]
fn create_event_checks_auth_before_the_allowlist() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let squatter = Address::generate(&env);

    // Without the caller's signature the call fails with a host auth error,
    // not `OrganizerNotApproved`, so an unauthenticated caller learns nothing
    // about allowlist membership (see the ordering note on the contract).
    env.mock_auths(&[]);
    let result = try_create(&env, &client, &squatter, 1);
    assert!(matches!(result, Err(Err(_))));
}
