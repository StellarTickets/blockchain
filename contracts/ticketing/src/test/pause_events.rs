//! Tests for pause and unpause events (Issue #492).

use super::*;
use soroban_sdk::testutils::Events as _;

fn paused_event_count(env: &Env, client: &TicketingContractClient, admin: &Address) -> usize {
    let expected = ContractPaused {
        admin: admin.clone(),
    }
    .to_xdr(env, &client.address);
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count()
}

fn unpaused_event_count(env: &Env, client: &TicketingContractClient, admin: &Address) -> usize {
    let expected = ContractUnpaused {
        admin: admin.clone(),
    }
    .to_xdr(env, &client.address);
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count()
}

#[test]
fn pause_emits_contract_paused_event() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    client.pause(&admin);

    assert_eq!(paused_event_count(&env, &client, &admin), 1);
    assert!(client.is_paused());
}

#[test]
fn unpause_emits_contract_unpaused_event() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    // Pause first
    client.pause(&admin);

    // Now unpause
    client.unpause(&admin);

    assert_eq!(unpaused_event_count(&env, &client, &admin), 1);
    assert!(!client.is_paused());
}

#[test]
fn multiple_pause_cycles_emit_events_each_time() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    // First pause
    client.pause(&admin);
    assert_eq!(paused_event_count(&env, &client, &admin), 1);

    // First unpause
    client.unpause(&admin);
    assert_eq!(unpaused_event_count(&env, &client, &admin), 1);

    // Second pause
    client.pause(&admin);
    // The latest invocation shows only the most recent event in events().all()
    assert_eq!(paused_event_count(&env, &client, &admin), 1);

    // Second unpause
    client.unpause(&admin);
    assert_eq!(unpaused_event_count(&env, &client, &admin), 1);
}

#[test]
fn pause_while_already_paused_still_emits_event() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    // First pause
    client.pause(&admin);

    // Pause again
    client.pause(&admin);

    // The latest invocation (second pause) should emit an event
    assert_eq!(paused_event_count(&env, &client, &admin), 1);
    assert!(client.is_paused());
}

#[test]
fn unpause_while_already_unpaused_still_emits_event() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    // Contract starts unpaused, so just call unpause
    client.unpause(&admin);

    // Should emit an event even though it was already unpaused
    assert_eq!(unpaused_event_count(&env, &client, &admin), 1);
    assert!(!client.is_paused());
}
