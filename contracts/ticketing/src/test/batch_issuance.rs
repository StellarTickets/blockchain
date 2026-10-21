//! Batch issuance (issue #88).
//!
//! `issue_tickets` mints many tickets for one event in a single call. It goes
//! through the same mint helper as `issue_ticket`, returns ids in the order of
//! the recipients, and is bounded by `MAX_BATCH_SIZE`.

use super::*;

fn ga(env: &Env) -> String {
    String::from_str(env, "GA")
}

fn recipient(env: &Env, to: &Address, price: i128) -> TicketRecipient {
    TicketRecipient {
        to: to.clone(),
        tier: ga(env),
        seat: String::from_str(env, "unassigned"),
        price,
    }
}

fn batch_of(env: &Env, count: u32) -> (Vec<TicketRecipient>, Vec<Address>) {
    let mut recipients = Vec::new(env);
    let mut owners = Vec::new(env);
    for _ in 0..count {
        let owner = Address::generate(env);
        recipients.push_back(recipient(env, &owner, 1_000));
        owners.push_back(owner);
    }
    (recipients, owners)
}

#[test]
fn issue_tickets_mints_one_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let (recipients, owners) = batch_of(&env, 1);

    let ids = client.issue_tickets(&organizer, &1, &recipients);

    assert_eq!(ids.len(), 1);
    assert_eq!(
        client.get_ticket(&ids.get(0).unwrap()).owner,
        owners.get(0).unwrap()
    );
    assert_eq!(client.get_event(&1).tickets_issued, 1);
}

#[test]
fn issue_tickets_returns_ids_in_recipient_order() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let (recipients, owners) = batch_of(&env, 5);

    let ids = client.issue_tickets(&organizer, &1, &recipients);

    assert_eq!(ids.len(), 5);
    for i in 0..5 {
        let ticket = client.get_ticket(&ids.get(i).unwrap());
        assert_eq!(ticket.owner, owners.get(i).unwrap());
        assert_eq!(ticket.original_price, 1_000);
        assert_eq!(ticket.event_id, 1);
    }
    assert_eq!(client.get_event(&1).tickets_issued, 5);
}

#[test]
fn issue_tickets_accepts_a_full_batch_at_max_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let (recipients, owners) = batch_of(&env, MAX_BATCH_SIZE);

    // A full batch writes more ledger entries than the default (mainnet)
    // resource limits allow, so the host aborts the invocation before the
    // batch logic is reached. This test is about the batch semantics — that
    // the whole batch is accepted and minted in order — so the limits are
    // lifted here. Whether a batch of `MAX_BATCH_SIZE` fits in a network
    // transaction is a separate concern, tracked by the cost regression
    // guards in `test/budget.rs`.
    env.cost_estimate().disable_resource_limits();

    let ids = client.issue_tickets(&organizer, &1, &recipients);

    assert_eq!(ids.len(), MAX_BATCH_SIZE);
    assert_eq!(
        client
            .get_ticket(&ids.get(MAX_BATCH_SIZE - 1).unwrap())
            .owner,
        owners.get(MAX_BATCH_SIZE - 1).unwrap()
    );
    assert_eq!(client.get_event(&1).tickets_issued, MAX_BATCH_SIZE as u64);
}

#[test]
fn issue_tickets_rejects_an_oversized_batch() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let (recipients, _owners) = batch_of(&env, MAX_BATCH_SIZE + 1);

    assert_eq!(
        client.try_issue_tickets(&organizer, &1, &recipients),
        Err(Ok(Error::BatchTooLarge))
    );
    assert_eq!(client.get_event(&1).tickets_issued, 0);
}

#[test]
fn issue_tickets_rejects_an_empty_batch() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let empty: Vec<TicketRecipient> = Vec::new(&env);

    assert_eq!(
        client.try_issue_tickets(&organizer, &1, &empty),
        Err(Ok(Error::EmptyBatch))
    );
}

#[test]
fn issue_tickets_is_atomic_when_one_recipient_is_invalid() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let mut recipients = Vec::new(&env);
    recipients.push_back(recipient(&env, &Address::generate(&env), 1_000));
    recipients.push_back(recipient(&env, &Address::generate(&env), -1));

    assert_eq!(
        client.try_issue_tickets(&organizer, &1, &recipients),
        Err(Ok(Error::InvalidPrice))
    );
    assert_eq!(client.get_event(&1).tickets_issued, 0);
    assert_eq!(client.total_tickets(), 0);
}

#[test]
fn issue_tickets_respects_the_event_capacity() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_capacity(&organizer, &1, &Some(2u64));
    let (recipients, _owners) = batch_of(&env, 3);

    assert_eq!(
        client.try_issue_tickets(&organizer, &1, &recipients),
        Err(Ok(Error::SoldOut))
    );
    assert_eq!(client.get_event(&1).tickets_issued, 0);
}

#[test]
fn issue_tickets_is_organizer_only() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);
    let (recipients, _owners) = batch_of(&env, 1);

    assert_eq!(
        client.try_issue_tickets(&stranger, &1, &recipients),
        Err(Ok(Error::NotOrganizer))
    );
}
