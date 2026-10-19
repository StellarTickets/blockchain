//! Two-step admin transfer (issue #100).

use super::*;

#[test]
fn accepted_transfer_moves_admin_rights_to_the_nominee() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_admin = Address::generate(&env);
    let newcomer = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    client.approve_organizer(&new_admin, &newcomer);
    assert_eq!(
        client.try_approve_organizer(&admin, &newcomer),
        Err(Ok(Error::NotAdmin))
    );
}

#[test]
fn proposal_alone_does_not_change_the_admin() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_admin = Address::generate(&env);
    let newcomer = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);

    assert_eq!(
        client.try_approve_organizer(&new_admin, &newcomer),
        Err(Ok(Error::NotAdmin))
    );
    client.approve_organizer(&admin, &newcomer);
}

#[test]
fn accept_without_a_pending_transfer_fails() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let new_admin = Address::generate(&env);

    assert_eq!(
        client.try_accept_admin(&new_admin),
        Err(Ok(Error::NoPendingAdmin))
    );
}

#[test]
fn only_the_nominee_can_accept() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);

    assert_eq!(
        client.try_accept_admin(&stranger),
        Err(Ok(Error::NotPendingAdmin))
    );
    // The rejected attempt left the nomination in place.
    client.accept_admin(&new_admin);
}

#[test]
fn a_new_proposal_replaces_the_pending_nominee() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    client.propose_admin(&admin, &first);
    client.propose_admin(&admin, &second);

    assert_eq!(
        client.try_accept_admin(&first),
        Err(Ok(Error::NotPendingAdmin))
    );
    client.accept_admin(&second);
}

#[test]
fn accepting_clears_the_pending_transfer() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_admin = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    assert_eq!(
        client.try_accept_admin(&new_admin),
        Err(Ok(Error::NoPendingAdmin))
    );
}

#[test]
fn non_admin_cannot_propose() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let intruder = Address::generate(&env);
    let new_admin = Address::generate(&env);

    assert_eq!(
        client.try_propose_admin(&intruder, &new_admin),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(
        client.try_accept_admin(&new_admin),
        Err(Ok(Error::NoPendingAdmin))
    );
}
