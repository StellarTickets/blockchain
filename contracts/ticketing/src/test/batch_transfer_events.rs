//! Tests for batch transfer events (Issue #460).

use super::*;

#[test]
fn transfer_batch_emits_ticket_transferred_events() {
    // Issue #460: transfer_batch should emit one TicketTransferred event per ticket.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Issue 3 tickets to the owner
    let ticket_id_1 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let ticket_id_2 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let ticket_id_3 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let ticket_ids = soroban_sdk::vec![&env, ticket_id_1, ticket_id_2, ticket_id_3];

    // Transfer batch
    client.transfer_batch(&owner, &ticket_ids, &recipient);

    // Count TicketTransferred events
    let events = env.events().all();
    let ticket_transferred_events: Vec<_> = events
        .iter()
        .filter(|e| {
            if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
                sym.to_string() == "TicketTransferred"
            } else {
                false
            }
        })
        .collect();

    // Should have 3 TicketTransferred events (one per ticket)
    assert_eq!(
        ticket_transferred_events.len(),
        3,
        "Should emit 3 TicketTransferred events for 3 tickets"
    );

    // Verify each ticket_id appears in an event
    let event_ticket_ids: Vec<u64> = ticket_transferred_events
        .iter()
        .map(|e| {
            if let soroban_sdk::Val::U64(id) = e.topics[1] {
                id
            } else {
                panic!("Expected ticket_id in event topics");
            }
        })
        .collect();

    assert!(event_ticket_ids.contains(&ticket_id_1));
    assert!(event_ticket_ids.contains(&ticket_id_2));
    assert!(event_ticket_ids.contains(&ticket_id_3));
}

#[test]
fn transfer_batch_with_single_ticket_emits_one_event() {
    // Test that a batch transfer of 1 ticket emits 1 event.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);

    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let ticket_ids = soroban_sdk::vec![&env, ticket_id];

    client.transfer_batch(&owner, &ticket_ids, &recipient);

    // Count TicketTransferred events
    let events = env.events().all();
    let ticket_transferred_count = events
        .iter()
        .filter(|e| {
            if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
                sym.to_string() == "TicketTransferred"
            } else {
                false
            }
        })
        .count();

    assert_eq!(
        ticket_transferred_count, 1,
        "Should emit 1 TicketTransferred event"
    );
}

#[test]
fn transfer_batch_events_match_individual_transfers() {
    // Issue #460: Batch transfers should emit the same events as individual transfers.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner1 = Address::generate(&env);
    let owner2 = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Create tickets for individual transfers
    let ticket_id_1 = issue_sample_ticket(&env, &client, &organizer, 1, &owner1, 1_000);
    let ticket_id_2 = issue_sample_ticket(&env, &client, &organizer, 1, &owner1, 1_000);

    // Create tickets for batch transfer
    let ticket_id_3 = issue_sample_ticket(&env, &client, &organizer, 1, &owner2, 1_000);
    let ticket_id_4 = issue_sample_ticket(&env, &client, &organizer, 1, &owner2, 1_000);

    // Individual transfers
    client.transfer_ticket(&owner1, &ticket_id_1, &recipient);
    client.transfer_ticket(&owner1, &ticket_id_2, &recipient);

    // Batch transfer
    let ticket_ids = soroban_sdk::vec![&env, ticket_id_3, ticket_id_4];
    client.transfer_batch(&owner2, &ticket_ids, &recipient);

    // Count TicketTransferred events (should be 4 total: 2 from individual, 2 from batch)
    let events = env.events().all();
    let ticket_transferred_count = events
        .iter()
        .filter(|e| {
            if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
                sym.to_string() == "TicketTransferred"
            } else {
                false
            }
        })
        .count();

    assert_eq!(
        ticket_transferred_count, 4,
        "Should emit 4 TicketTransferred events total"
    );

    // All tickets should be owned by recipient
    assert_eq!(client.get_ticket(&ticket_id_1).owner, recipient);
    assert_eq!(client.get_ticket(&ticket_id_2).owner, recipient);
    assert_eq!(client.get_ticket(&ticket_id_3).owner, recipient);
    assert_eq!(client.get_ticket(&ticket_id_4).owner, recipient);
}

#[test]
fn transfer_batch_with_max_batch_size_emits_all_events() {
    // Test that batch transfers at the maximum size emit correct number of events.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);

    // MAX_BATCH_SIZE is 100, create that many tickets
    let mut ticket_ids = soroban_sdk::Vec::new(&env);
    for _ in 0..100 {
        let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
        ticket_ids.push_back(ticket_id);
    }

    client.transfer_batch(&owner, &ticket_ids, &recipient);

    // Count TicketTransferred events
    let events = env.events().all();
    let ticket_transferred_count = events
        .iter()
        .filter(|e| {
            if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
                sym.to_string() == "TicketTransferred"
            } else {
                false
            }
        })
        .count();

    assert_eq!(
        ticket_transferred_count, 100,
        "Should emit 100 TicketTransferred events for max batch"
    );
}
