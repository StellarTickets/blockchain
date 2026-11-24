//! `EventConfigChanged` events from the event-level setters (issue #516).
//!
//! Indexers mirror event config from these events. Each successful setter
//! publishes one event whose `field` names the setting, and a rejected call
//! publishes nothing.

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::{Event as _, Symbol};

fn config_event_count(
    env: &Env,
    client: &TicketingContractClient,
    event_id: u64,
    field: &str,
) -> usize {
    let expected = EventConfigChanged {
        event_id,
        field: Symbol::new(env, field),
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
fn capacity_setter_publishes_capacity_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_capacity(&organizer, &1, &Some(50));

    assert_eq!(config_event_count(&env, &client, 1, "capacity"), 1);
}

#[test]
fn check_in_window_setter_publishes_check_in_window_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_check_in_window(&organizer, &1, &Some(600), &Some(20_000));

    assert_eq!(config_event_count(&env, &client, 1, "check_in_window"), 1);
}

#[test]
fn ticket_expiry_setter_publishes_ticket_expiry_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_ticket_expiry(&organizer, &1, &Some(30_000));

    assert_eq!(config_event_count(&env, &client, 1, "ticket_expiry"), 1);
}

#[test]
fn max_reentries_setter_publishes_max_reentries_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_max_reentries(&organizer, &1, &2);

    assert_eq!(config_event_count(&env, &client, 1, "max_reentries"), 1);
}

#[test]
fn sale_window_setter_publishes_sale_window_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_sale_window(&organizer, &1, &Some(100), &Some(900));

    assert_eq!(config_event_count(&env, &client, 1, "sale_window"), 1);
}

#[test]
fn transferable_setter_publishes_transferable_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_transferable(&organizer, &1, &false);

    assert_eq!(config_event_count(&env, &client, 1, "transferable"), 1);
}

#[test]
fn payment_token_setter_publishes_payment_token_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_event_payment_token(&organizer, &1, &None);

    assert_eq!(config_event_count(&env, &client, 1, "payment_token"), 1);
}

#[test]
fn royalty_splits_setter_publishes_royalty_splits_changed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_royalty_splits(&organizer, &1, &None);

    assert_eq!(config_event_count(&env, &client, 1, "royalty_splits"), 1);
}

#[test]
fn rejected_setter_publishes_no_config_changed_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // A zero capacity is invalid, so the setter fails before saving anything.
    assert_eq!(
        client.try_set_event_capacity(&organizer, &1, &Some(0)),
        Err(Ok(Error::InvalidCapacity))
    );

    assert_eq!(config_event_count(&env, &client, 1, "capacity"), 0);
}

#[test]
fn config_event_is_keyed_by_event_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &organizer, 2);

    client.set_event_max_reentries(&organizer, &2, &1);

    assert_eq!(config_event_count(&env, &client, 2, "max_reentries"), 1);
    assert_eq!(config_event_count(&env, &client, 1, "max_reentries"), 0);
}
