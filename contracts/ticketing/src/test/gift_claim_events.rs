//! Tests for gift claim lifecycle events (Issue #462).

use super::*;

#[test]
fn create_gift_claim_emits_gift_claim_created_event() {
    // Issue #462: create_gift_claim should emit GiftClaimCreated event.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let secret = Bytes::from_slice(&env, b"my-secret");
    let secret_hash = env.crypto().sha256(&secret).to_bytes();
    let expires_at = 5_000;

    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &expires_at);

    // Verify the event was emitted
    let events = env.events().all();
    let gift_claim_created_event = events.iter().find(|e| {
        if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
            sym.to_string() == "GiftClaimCreated"
        } else {
            false
        }
    });

    assert!(
        gift_claim_created_event.is_some(),
        "GiftClaimCreated event should be emitted"
    );

    // Verify event data
    let event = gift_claim_created_event.unwrap();
    assert_eq!(event.topics.len(), 2);
    // Topic 1 should be the ticket_id
    if let soroban_sdk::Val::U64(id) = event.topics[1] {
        assert_eq!(id, ticket_id);
    } else {
        panic!("Expected ticket_id in event topics");
    }
}

#[test]
fn claim_gift_emits_gift_claimed_event() {
    // Issue #462: claim_gift should emit GiftClaimed event.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    let secret = Bytes::from_slice(&env, b"my-secret");
    let secret_hash = env.crypto().sha256(&secret).to_bytes();
    let expires_at = 5_000;

    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &expires_at);

    env.ledger().with_mut(|li| {
        li.timestamp = 1_000;
    });

    client.claim_gift(&recipient, &ticket_id, &secret);

    // Verify the event was emitted
    let events = env.events().all();
    let gift_claimed_event = events.iter().find(|e| {
        if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
            sym.to_string() == "GiftClaimed"
        } else {
            false
        }
    });

    assert!(
        gift_claimed_event.is_some(),
        "GiftClaimed event should be emitted"
    );

    // Verify event data
    let event = gift_claimed_event.unwrap();
    assert_eq!(event.topics.len(), 2);
    // Topic 1 should be the ticket_id
    if let soroban_sdk::Val::U64(id) = event.topics[1] {
        assert_eq!(id, ticket_id);
    } else {
        panic!("Expected ticket_id in event topics");
    }

    // Verify ownership transfer
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, recipient);
}

#[test]
fn gift_claim_lifecycle_events_documented() {
    // Issue #462: Ensure events exist and are documented (compile-time check).
    // This test verifies that the event structs exist and can be constructed.
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);

    let _gift_claim_created = GiftClaimCreated {
        ticket_id: 1,
        from: owner.clone(),
        expires_at: 5_000,
    };

    let _gift_claimed = GiftClaimed {
        ticket_id: 1,
        from: owner,
        recipient,
    };

    // If this compiles, the events exist and are properly exported.
}

#[test]
fn multiple_gift_claims_emit_multiple_events() {
    // Test that multiple gift claims each emit their own event.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let owner1 = Address::generate(&env);
    let owner2 = Address::generate(&env);
    let ticket_id_1 = issue_sample_ticket(&env, &client, &organizer, 1, &owner1, 1_000);
    let ticket_id_2 = issue_sample_ticket(&env, &client, &organizer, 1, &owner2, 1_000);

    let secret1 = Bytes::from_slice(&env, b"secret1");
    let secret2 = Bytes::from_slice(&env, b"secret2");
    let hash1 = env.crypto().sha256(&secret1).to_bytes();
    let hash2 = env.crypto().sha256(&secret2).to_bytes();

    client.create_gift_claim(&owner1, &ticket_id_1, &hash1, &5_000);
    client.create_gift_claim(&owner2, &ticket_id_2, &hash2, &5_000);

    // Count GiftClaimCreated events
    let events = env.events().all();
    let gift_claim_created_count = events
        .iter()
        .filter(|e| {
            if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
                sym.to_string() == "GiftClaimCreated"
            } else {
                false
            }
        })
        .count();

    assert_eq!(
        gift_claim_created_count, 2,
        "Should emit 2 GiftClaimCreated events"
    );

    // Now claim both
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000;
    });

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    client.claim_gift(&recipient1, &ticket_id_1, &secret1);
    client.claim_gift(&recipient2, &ticket_id_2, &secret2);

    // Count GiftClaimed events
    let events = env.events().all();
    let gift_claimed_count = events
        .iter()
        .filter(|e| {
            if let soroban_sdk::Val::Symbol(sym) = &e.topics[0] {
                sym.to_string() == "GiftClaimed"
            } else {
                false
            }
        })
        .count();

    assert_eq!(gift_claimed_count, 2, "Should emit 2 GiftClaimed events");
}
