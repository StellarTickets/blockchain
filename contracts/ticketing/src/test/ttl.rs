//! Tests for expiring TTL behaviour (issue #139).
//!
//! Every persistent entry in this contract is created with
//! `LEDGER_BUMP` (535_679 ledgers, ~30 days) of life and is meant to be
//! refreshed on each write, with `LEDGER_THRESHOLD` (500_000) as the
//! point below which a refresh is due. On mainnet an entry that is not
//! refreshed before its `live_until_ledger` passes is archived: the value
//! is gone and the entry reads back as absent.
//!
//! The failure these tests guard against is quiet. The contract does not
//! return an error when an entry ages out -- the read simply reports
//! `EventNotFound` or `TicketNotFound`, and callers cannot tell that apart
//! from a genuinely missing record. So each test drives the ledger past
//! the original expiry, performs a legitimate write, and asserts that the
//! write pushed the entry's TTL back out. `extend_ttl(threshold, bump)`
//! only extends when the remaining life is *below* the threshold, which is
//! why the ledger is advanced first: at the starting sequence there is
//! nothing to refresh and the test would pass for the wrong reason.
//!
//! Note that the test host does not itself run archival, so an entry that
//! really has expired is still returned by `get`. These tests therefore
//! assert on TTL values, which is what actually governs the outcome on a
//! real network, rather than on reads failing.

use super::*;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

/// A sequence number at which a freshly created entry needs refreshing: just
/// past the bump, so remaining life (4_095) is below the threshold.
const STALE: u32 = 535_680;

/// A sequence number well past the expiry an entry was created with.
const LONG_AFTER: u32 = 900_000;

fn event_ttl(env: &Env, client: &TicketingContractClient, event_id: u64) -> u32 {
    let key = DataKey::Event(event_id);
    env.as_contract(&client.address, || env.storage().persistent().get_ttl(&key))
}

fn ticket_ttl(env: &Env, client: &TicketingContractClient, ticket_id: u64) -> u32 {
    let key = DataKey::Ticket(ticket_id);
    env.as_contract(&client.address, || env.storage().persistent().get_ttl(&key))
}

fn instance_ttl(env: &Env, client: &TicketingContractClient) -> u32 {
    env.as_contract(&client.address, || env.storage().instance().get_ttl())
}

fn issue(env: &Env, client: &TicketingContractClient, organizer: &Address) -> u64 {
    let owner = Address::generate(env);
    client.issue_ticket(
        organizer,
        &1,
        &owner,
        &String::from_str(env, "GA"),
        &String::from_str(env, "unassigned"),
        &1_000i128,
    )
}

/// Enabling escrow rewrites the event, so it must push the event's TTL back
/// out. It did not: it wrote the entry and left the original expiry in
/// place, which meant an event could be switched to escrow just before
/// ageing out and then become unreadable.
#[test]
fn enable_escrow_refreshes_the_event_ttl() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // `enable_escrow` refuses once tickets are issued, so the ticket has to
    // wait until after the write under test.
    env.ledger().set_sequence_number(STALE);
    assert!(
        event_ttl(&env, &client, 1) < LEDGER_THRESHOLD,
        "precondition: the event is due a refresh"
    );

    client.enable_escrow(&organizer, &1, &500u32);

    assert!(
        event_ttl(&env, &client, 1) > LEDGER_THRESHOLD,
        "enable_escrow wrote the event but did not extend it"
    );

    // The event is still readable long after its original expiry.
    let ticket_id = issue(&env, &client, &organizer);
    env.ledger().set_sequence_number(LONG_AFTER);
    assert!(client.try_get_event(&1).is_ok());
    assert!(client.try_get_ticket(&ticket_id).is_ok());
    assert!(client.get_event(&1).escrow_enabled);
}

/// The same for a primary sale that credits escrow: the buyer's tokens are
/// in the contract, so the event recording them has to stay alive.
#[test]
fn escrowed_primary_sale_refreshes_the_event_ttl() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    env.ledger().set_sequence_number(STALE);
    assert!(event_ttl(&env, &client, 1) < LEDGER_THRESHOLD);

    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &2_000i128,
    );

    assert_eq!(client.get_event(&1).escrow_balance, 2_000);
    assert!(
        event_ttl(&env, &client, 1) > LEDGER_THRESHOLD,
        "the escrowed credit did not extend the event"
    );

    // The organizer can still be paid long afterwards, which is the point:
    // without the refresh this read would be `EventNotFound` on mainnet and
    // the escrowed balance would be stranded.
    env.ledger().set_sequence_number(LONG_AFTER);
    let released = client.try_release_escrow(&organizer, &1);
    assert!(released.is_ok(), "release_escrow lost the event");
}

/// Releasing escrow rewrites the event too, and is the last point at which
/// the balance can be corrected.
#[test]
fn release_escrow_refreshes_the_event_ttl() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &2_000i128,
    );

    env.ledger().set_sequence_number(LONG_AFTER);
    client.release_escrow(&organizer, &1);

    assert_eq!(client.get_event(&1).escrow_balance, 0);
    assert!(
        event_ttl(&env, &client, 1) > LEDGER_THRESHOLD,
        "release_escrow wrote the event but did not extend it"
    );
    assert!(client.try_get_event(&1).is_ok());
}

/// Setting a per-event payment token rewrites the event.
#[test]
fn setting_a_payment_token_refreshes_the_event_ttl() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let token_admin = Address::generate(&env);
    let override_token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let override_client = StellarAssetClient::new(&env, &override_token.address());

    env.ledger().set_sequence_number(STALE);
    client.set_event_payment_token(&organizer, &1, &Some(override_client.address.clone()));

    assert!(
        event_ttl(&env, &client, 1) > LEDGER_THRESHOLD,
        "set_event_payment_token wrote the event but did not extend it"
    );
    env.ledger().set_sequence_number(LONG_AFTER);
    assert!(client.try_get_event(&1).is_ok());
}

/// A ticket write refreshes the ticket entry.
#[test]
fn writing_a_ticket_refreshes_its_ttl() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue(&env, &client, &organizer);

    env.ledger().set_sequence_number(STALE);
    assert!(ticket_ttl(&env, &client, ticket_id) < LEDGER_THRESHOLD);

    client.check_in(&organizer, &ticket_id);

    assert!(
        ticket_ttl(&env, &client, ticket_id) > LEDGER_THRESHOLD,
        "check_in wrote the ticket but did not extend it"
    );

    env.ledger().set_sequence_number(LONG_AFTER);
    assert!(client.try_get_ticket(&ticket_id).is_ok());
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Used);
}

/// A resale write refreshes the ticket entry as well, on the buyer's side of
/// the trade rather than the organizer's.
#[test]
fn buying_a_resale_refreshes_the_ticket_ttl() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );
    client.list_for_resale(&seller, &ticket_id, &1_100i128);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    env.ledger().set_sequence_number(STALE);
    assert!(ticket_ttl(&env, &client, ticket_id) < LEDGER_THRESHOLD);

    client.buy_resale(&buyer, &ticket_id);

    assert!(
        ticket_ttl(&env, &client, ticket_id) > LEDGER_THRESHOLD,
        "buy_resale wrote the ticket but did not extend it"
    );
    env.ledger().set_sequence_number(LONG_AFTER);
    assert_eq!(client.get_ticket(&ticket_id).owner, buyer);
}

/// The contract's instance storage has to stay alive too: every entry point
/// reads it, so an expired instance makes the whole contract unusable rather
/// than just one record unreadable. Every mutating entry point calls
/// `extend_instance_ttl`, and this pins that it keeps happening.
#[test]
fn a_write_refreshes_the_instance_ttl() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    env.ledger().set_sequence_number(STALE);
    assert!(instance_ttl(&env, &client) < LEDGER_THRESHOLD);

    let ticket_id = issue(&env, &client, &organizer);

    assert!(
        instance_ttl(&env, &client) > LEDGER_THRESHOLD,
        "issue_ticket did not refresh the instance"
    );
    assert!(client.try_get_event(&1).is_ok());
    assert!(client.try_get_ticket(&ticket_id).is_ok());
}

/// A read must not be what keeps the contract alive.
///
/// This is the counterpart to the read-does-not-write decision recorded in
/// `tickets.rs`: reads deliberately do not extend TTL, so the instance is
/// refreshed only by writes. That is a real operational constraint -- a
/// contract with no activity for a full bump window is gone -- and it is
/// worth being explicit about rather than discovering later.
#[test]
fn reads_alone_do_not_refresh_the_instance_ttl() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue(&env, &client, &organizer);

    env.ledger().set_sequence_number(STALE);
    let before = instance_ttl(&env, &client);

    // A burst of reads, which is what a busy front end does.
    for _ in 0..5 {
        let _ = client.try_get_event(&1);
        let _ = client.try_get_ticket(&ticket_id);
    }

    assert_eq!(
        instance_ttl(&env, &client),
        before,
        "a read extended the instance TTL"
    );

    // A write is what brings it back.
    client.check_in(&organizer, &ticket_id);
    assert!(instance_ttl(&env, &client) > LEDGER_THRESHOLD);
}
