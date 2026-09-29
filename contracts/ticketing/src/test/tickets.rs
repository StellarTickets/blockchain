use super::*;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

#[test]
fn category_vocabulary_has_other_and_standard_labels() {
    assert!(matches!(Category::Other, Category::Other));
    assert_eq!(CATEGORY_CONCERT, "concert");
    assert_eq!(CATEGORY_OTHER, "other");
}

#[test]
fn issues_and_verifies_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &5_000i128,
    );

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.original_price, 5_000);

    let event = client.get_event(&1);
    assert_eq!(event.tickets_issued, 1);
}

#[test]
fn public_calls_report_not_initialized_before_setup() {
    let (env, client, token, _admin, organizer) = setup_uninitialized();
    let buyer = Address::generate(&env);

    let create_event = client.try_create_event(
        &organizer,
        &1,
        &String::from_str(&env, "Uninitialized"),
        &String::from_str(&env, "concert"),
        &12_000u32,
        &500u32,
        &10_000u64,
        &100u64,
        &200u64,
    );
    assert_eq!(create_event, Err(Ok(Error::NotInitialized)));

    let issue_ticket = client.try_issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );
    assert_eq!(issue_ticket, Err(Ok(Error::NotInitialized)));

    assert_eq!(client.try_get_event(&1), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_get_ticket(&1), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_token_decimals(), Err(Ok(Error::NotInitialized)));

    client.initialize(&Address::generate(&env), &token);
    assert_eq!(client.try_get_ticket(&1), Err(Ok(Error::TicketNotFound)));
}

#[test]
fn tickets_are_stored_in_the_packed_representation() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    let key = DataKey::Ticket(ticket_id);
    let contract_address = client.address.clone();
    let stored: StoredTicket = env.as_contract(&contract_address, || {
        env.storage().persistent().get(&key).unwrap()
    });
    assert_eq!(stored.lifecycle, 0);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, owner);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.transfers, 0);
}

#[test]
fn reading_a_ticket_does_not_extend_its_ttl() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = 40_000);
    let contract_address = client.address.clone();
    let key = DataKey::Ticket(ticket_id);
    let ticket_ttl_before = env.as_contract(&contract_address, || {
        env.storage().persistent().get_ttl(&key)
    });
    let instance_ttl_before =
        env.as_contract(&contract_address, || env.storage().instance().get_ttl());

    client.get_ticket(&ticket_id);

    let ticket_ttl_after = env.as_contract(&contract_address, || {
        env.storage().persistent().get_ttl(&key)
    });
    let instance_ttl_after =
        env.as_contract(&contract_address, || env.storage().instance().get_ttl());

    assert_eq!(ticket_ttl_after, ticket_ttl_before);
    assert_eq!(instance_ttl_after, instance_ttl_before);
}

/// Read paths deliberately do not bump TTL.
///
/// An earlier draft of this test asserted the opposite, and that assertion is
/// what the broken merge left behind. Bumping a TTL is a state write: doing it
/// inside `get_event`/`get_ticket` would make every read cost fees and consume
/// ledger write budget, and it would mean a query mutates state. The contract
/// instead refreshes TTL on the write paths that matter, via `save_ticket` and
/// the `extend_ttl` calls next to each mutation. These tests pin that
/// decision so a future change has to argue with it rather than reintroduce it.
#[test]
fn extend_ticket_ttl_renews_ticket_without_changing_state() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = 40_000);
    let key = DataKey::Ticket(ticket_id);
    let contract_address = client.address.clone();
    let ticket_ttl_before = env.as_contract(&contract_address, || {
        env.storage().persistent().get_ttl(&key)
    });
    let ticket_before = client.get_ticket(&ticket_id);

    client.extend_ticket_ttl(&ticket_id);

    let ticket_ttl_after = env.as_contract(&contract_address, || {
        env.storage().persistent().get_ttl(&key)
    });
    assert!(ticket_ttl_after > ticket_ttl_before);
    assert_eq!(client.get_ticket(&ticket_id), ticket_before);
}

#[test]
fn reading_an_event_does_not_extend_its_ttl() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = 40_000);
    let contract_address = client.address.clone();
    let instance_ttl_before =
        env.as_contract(&contract_address, || env.storage().instance().get_ttl());

    client.get_event(&1);

    let instance_ttl_after =
        env.as_contract(&contract_address, || env.storage().instance().get_ttl());

    assert_eq!(instance_ttl_after, instance_ttl_before);
}

#[test]
fn ticket_ids_are_sequential_and_unique_across_fifty_mints() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    for expected_id in 0..50u64 {
        let owner = Address::generate(&env);
        let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

        assert_eq!(ticket_id, expected_id);
        assert_eq!(client.verify_ticket(&ticket_id).owner, owner);
    }
}

#[test]
fn verify_ticket_reports_each_ticket_state() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let valid_owner = Address::generate(&env);
    let resale_owner = Address::generate(&env);
    let used_owner = Address::generate(&env);
    let revoked_owner = Address::generate(&env);

    let valid_id = issue_sample_ticket(&env, &client, &organizer, 1, &valid_owner, 1_000);
    let resale_id = issue_sample_ticket(&env, &client, &organizer, 1, &resale_owner, 1_000);
    let used_id = issue_sample_ticket(&env, &client, &organizer, 1, &used_owner, 1_000);
    let revoked_id = issue_sample_ticket(&env, &client, &organizer, 1, &revoked_owner, 1_000);

    client.list_for_resale(&resale_owner, &resale_id, &1_100);
    client.check_in(&organizer, &used_id);
    client.revoke_ticket(&organizer, &revoked_id);

    let cases = [
        (valid_id, TicketStatus::Valid),
        (resale_id, TicketStatus::Resale),
        (used_id, TicketStatus::Used),
        (revoked_id, TicketStatus::Revoked),
    ];

    for (ticket_id, expected_status) in cases {
        assert_eq!(client.verify_ticket(&ticket_id).status, expected_status);
    }
}

#[test]
fn get_ticket_reports_not_found_for_an_unknown_id() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let result = client.try_get_ticket(&999);
    assert_eq!(result, Err(Ok(Error::TicketNotFound)));
}

#[test]
fn is_valid_matches_owner_and_ticket_status() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let other_owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    assert!(client.is_valid(&ticket_id, &owner));
    assert!(!client.is_valid(&ticket_id, &other_owner));
    assert!(!client.is_valid(&999, &owner));

    client.check_in(&organizer, &ticket_id);
    assert!(!client.is_valid(&ticket_id, &owner));
}

#[test]
fn issue_ticket_rejects_a_negative_price() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let result = client.try_issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &-1i128,
    );
    assert_eq!(result, Err(Ok(Error::InvalidPrice)));
}

#[test]
fn issue_ticket_allows_a_zero_price_comp_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let comp_recipient = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &comp_recipient,
        &String::from_str(&env, "COMP"),
        &String::from_str(&env, "A1"),
        &0i128,
    );
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.original_price, 0);
    assert_eq!(ticket.owner, comp_recipient);
}

#[test]
fn non_organizer_cannot_issue_tickets_for_someone_elses_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let attacker = Address::generate(&env);
    let buyer = Address::generate(&env);
    let result = client.try_issue_ticket(
        &attacker,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    assert_eq!(result, Err(Ok(Error::NotOrganizer)));
}

#[test]
fn purchase_primary_pays_organizer_on_chain() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    assert_eq!(token.balance(&organizer), 2_000);
    assert_eq!(token.balance(&buyer), 8_000);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.original_price, 2_000);
}

#[test]
fn purchase_primary_increments_tickets_issued() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &1_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );
    assert_eq!(client.get_event(&1).tickets_issued, 1);
}

#[test]
fn issue_ticket_and_purchase_primary_share_the_id_counter() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let issued_owner = Address::generate(&env);
    let first_buyer = Address::generate(&env);
    let second_buyer = Address::generate(&env);

    let first_issued = issue_sample_ticket(&env, &client, &organizer, 1, &issued_owner, 0);
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &0);
    let first_purchased = client.purchase_primary(
        &first_buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );
    let second_issued = issue_sample_ticket(&env, &client, &organizer, 1, &issued_owner, 0);
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &0);
    let second_purchased = client.purchase_primary(
        &second_buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
    );

    assert_eq!(
        [
            first_issued,
            first_purchased,
            second_issued,
            second_purchased
        ],
        [0, 1, 2, 3]
    );
}

#[test]
fn purchase_primary_allows_a_free_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "FREE"), &0i128);
    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "FREE"),
        &String::from_str(&env, "GA"),
    );
    assert_eq!(client.get_ticket(&ticket_id).original_price, 0);
}

#[test]
fn purchase_primary_charges_the_organizers_tier_price_not_a_caller_supplied_one() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);

    // The purchase takes no price argument at all any more: the only way to
    // buy the tier is at the organizer's price, so underpayment is impossible.
    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    assert_eq!(token.balance(&organizer), 2_000);
    assert_eq!(token.balance(&buyer), 8_000);
    assert_eq!(client.get_ticket(&ticket_id).original_price, 2_000);
}

#[test]
fn purchase_primary_cannot_bypass_the_tier_price_with_a_paid_tier_requested_as_free() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);

    // Asking for a tier the organizer never priced cannot be used to pay 0.
    let result = client.try_purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "1"),
    );
    assert_eq!(result, Err(Ok(Error::TierPriceNotSet)));

    // Nothing moved and no ticket was minted.
    assert_eq!(token.balance(&buyer), 10_000);
    assert_eq!(client.get_event(&1).tickets_issued, 0);
}

#[test]
fn purchase_primary_fails_when_the_organizer_has_not_priced_the_tier() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    let result = client.try_purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );
    assert_eq!(result, Err(Ok(Error::TierPriceNotSet)));
    assert_eq!(token.balance(&buyer), 10_000);
}

#[test]
fn set_tier_price_is_organizer_only_and_rejects_negative_prices() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_set_tier_price(&stranger, &1, &String::from_str(&env, "GA"), &0i128),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &-1i128),
        Err(Ok(Error::InvalidPrice))
    );
}

#[test]
fn set_tier_price_updates_the_price_for_later_purchases_only() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let first_buyer = Address::generate(&env);
    let second_buyer = Address::generate(&env);
    token_asset.mint(&first_buyer, &10_000i128);
    token_asset.mint(&second_buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &1_000i128);
    let first_ticket = client.purchase_primary(
        &first_buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    // The organizer raises the price; the ticket already sold keeps its own
    // original_price (resale caps and refunds are derived from it).
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &1_500i128);
    let second_ticket = client.purchase_primary(
        &second_buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
    );

    assert_eq!(client.get_ticket(&first_ticket).original_price, 1_000);
    assert_eq!(client.get_ticket(&second_ticket).original_price, 1_500);
    assert_eq!(token.balance(&organizer), 2_500);
    assert_eq!(
        client.get_tier_price(&1, &String::from_str(&env, "GA")),
        1_500
    );
}

#[test]
fn transfer_moves_ownership() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let friend = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );

    client.transfer_ticket(&buyer, &ticket_id, &friend);
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, friend);

    let stale = client.try_transfer_ticket(&buyer, &ticket_id, &organizer);
    assert_eq!(stale, Err(Ok(Error::NotOwner)));
}

#[test]
fn seat_and_tier_survive_a_transfer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let friend = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "Row A Seat 1"),
        &10_000i128,
    );

    client.transfer_ticket(&buyer, &ticket_id, &friend);
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.tier, String::from_str(&env, "VIP"));
    assert_eq!(ticket.seat, String::from_str(&env, "Row A Seat 1"));
}

#[test]
fn direct_transfer_freezes_at_configured_window() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    // starts_at = 10_000, transfer_freeze_seconds = 100 -> freezes at timestamp 9_900
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let friend = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    env.ledger().set_timestamp(9_899);
    client.transfer_ticket(&buyer, &ticket_id, &friend);
    assert_eq!(client.get_ticket(&ticket_id).owner, friend);

    env.ledger().set_timestamp(9_900);
    let frozen = client.try_transfer_ticket(&friend, &ticket_id, &buyer);
    assert_eq!(frozen, Err(Ok(Error::TransfersFrozen)));
}

#[test]
fn transfer_batch_moves_all_tickets_atomically() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let friend = Address::generate(&env);

    let t1 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let t2 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
        &1_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(t1);
    batch.push_back(t2);

    client.transfer_batch(&buyer, &batch, &friend);
    assert_eq!(client.get_ticket(&t1).owner, friend);
    assert_eq!(client.get_ticket(&t2).owner, friend);
}

#[test]
fn transfer_batch_rejects_empty_or_oversized_batches() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let friend = Address::generate(&env);

    let empty = Vec::new(&env);
    let err_empty = client.try_transfer_batch(&buyer, &empty, &friend);
    assert_eq!(err_empty, Err(Ok(Error::EmptyBatch)));
}

#[test]
fn gift_claim_transfers_ticket_with_correct_secret() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    let secret_bytes = Bytes::from_array(&env, &[42u8; 32]);
    let secret_hash = env.crypto().sha256(&secret_bytes).to_bytes();

    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64);
    client.claim_gift(&recipient, &ticket_id, &secret_bytes);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, recipient);
    assert_eq!(ticket.status, TicketStatus::Valid);
}

#[test]
fn gift_claim_rejects_wrong_secret_and_expired_claim() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    let secret_bytes = Bytes::from_array(&env, &[42u8; 32]);
    let wrong_secret = Bytes::from_array(&env, &[99u8; 32]);
    let secret_hash = env.crypto().sha256(&secret_bytes).to_bytes();

    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64);

    let bad_secret = client.try_claim_gift(&recipient, &ticket_id, &wrong_secret);
    assert_eq!(bad_secret, Err(Ok(Error::InvalidSecret)));

    env.ledger().set_timestamp(5_000);
    let expired = client.try_claim_gift(&recipient, &ticket_id, &secret_bytes);
    assert_eq!(expired, Err(Ok(Error::GiftClaimExpired)));
}

#[test]
fn check_in_marks_used_and_rejects_reentry() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "A1"),
        &10_000i128,
    );

    client.check_in(&organizer, &ticket_id);
    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Used);

    let result = client.try_check_in(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::AlreadyUsed)));
}

/// Issue #132: Attempting to check in a ticket that is currently listed for resale
/// must be rejected with Error::ResaleListingActive until delisted.
#[test]
fn check_in_rejects_resale_listed_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    client.list_for_resale(&buyer, &ticket_id, &1_100i128);
    assert_eq!(client.get_ticket(&ticket_id).resale_price, 1_100);

    let result = client.try_check_in(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::ResaleListingActive)));

    client.cancel_resale(&buyer, &ticket_id);
    client.check_in(&organizer, &ticket_id);

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Used);
    assert_eq!(ticket.resale_price, 0);
}

#[test]
fn check_in_rejects_the_wrong_organizer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let attacker = Address::generate(&env);
    let result = client.try_check_in(&attacker, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotOrganizer)));
}

#[test]
fn check_in_batch_marks_all_tickets_used() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let t1 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let t2 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
        &1_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(t1);
    batch.push_back(t2);

    client.check_in_batch(&organizer, &batch);
    assert_eq!(client.get_ticket(&t1).status, TicketStatus::Used);
    assert_eq!(client.get_ticket(&t2).status, TicketStatus::Used);
}

/// Issue #132: check_in_batch rejects any ticket in Resale status.
#[test]
fn check_in_batch_rejects_resale_listed_tickets() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let t1 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let t2 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
        &1_000i128,
    );
    client.list_for_resale(&buyer, &t1, &1_100i128);

    let mut batch = Vec::new(&env);
    batch.push_back(t1);
    batch.push_back(t2);

    let result = client.try_check_in_batch(&organizer, &batch);
    assert_eq!(result, Err(Ok(Error::ResaleListingActive)));

    client.cancel_resale(&buyer, &t1);
    client.check_in_batch(&organizer, &batch);

    for ticket_id in [t1, t2] {
        let ticket = client.get_ticket(&ticket_id);
        assert_eq!(ticket.status, TicketStatus::Used);
        assert_eq!(ticket.resale_price, 0);
    }
}

#[test]
fn check_in_after_transfer_succeeds_for_new_owner() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let original_buyer = Address::generate(&env);
    let new_owner = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &original_buyer,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "Row A"),
        &5_000i128,
    );

    let ticket_before = client.get_ticket(&ticket_id);
    assert_eq!(ticket_before.owner, original_buyer);
    assert_eq!(ticket_before.status, TicketStatus::Valid);

    client.transfer_ticket(&original_buyer, &ticket_id, &new_owner);

    let ticket_transferred = client.get_ticket(&ticket_id);
    assert_eq!(ticket_transferred.owner, new_owner);
    assert_eq!(ticket_transferred.status, TicketStatus::Valid);

    client.check_in(&organizer, &ticket_id);

    let ticket_checked_in = client.get_ticket(&ticket_id);
    assert_eq!(ticket_checked_in.owner, new_owner);
    assert_eq!(ticket_checked_in.status, TicketStatus::Used);

    let reentry_result = client.try_check_in(&organizer, &ticket_id);
    assert_eq!(reentry_result, Err(Ok(Error::AlreadyUsed)));

    let third_party = Address::generate(&env);
    let transfer_after_use = client.try_transfer_ticket(&new_owner, &ticket_id, &third_party);
    assert_eq!(transfer_after_use, Err(Ok(Error::AlreadyUsed)));
}

#[test]
fn revoked_ticket_cannot_be_checked_in_or_transferred() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );

    client.revoke_ticket(&organizer, &ticket_id);

    let checkin_result = client.try_check_in(&organizer, &ticket_id);
    assert_eq!(checkin_result, Err(Ok(Error::Revoked)));

    let other = Address::generate(&env);
    let transfer_result = client.try_transfer_ticket(&buyer, &ticket_id, &other);
    assert_eq!(transfer_result, Err(Ok(Error::Revoked)));
}

#[test]
fn revoke_rejects_the_wrong_organizer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let attacker = Address::generate(&env);
    let result = client.try_revoke_ticket(&attacker, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotOrganizer)));
}

#[test]
fn revoke_permanently_blocks_resale_actions() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    client.revoke_ticket(&organizer, &ticket_id);
    let result = client.try_list_for_resale(&buyer, &ticket_id, &1_100i128);
    assert_eq!(result, Err(Ok(Error::Revoked)));
}

#[test]
fn revoke_with_refund_returns_payment_to_owner() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    assert_eq!(token.balance(&organizer), 2_000);
    assert_eq!(token.balance(&buyer), 8_000);

    token_asset.mint(&organizer, &5_000i128);
    client.revoke_with_refund(&organizer, &ticket_id, &true);

    assert_eq!(token.balance(&buyer), 10_000);
    assert_eq!(
        client.verify_ticket(&ticket_id).status,
        TicketStatus::Revoked
    );
}

#[test]
fn revoke_batch_revokes_all_tickets() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let t1 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let t2 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
        &1_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(t1);
    batch.push_back(t2);

    client.revoke_batch(&organizer, &batch);
    assert_eq!(client.get_ticket(&t1).status, TicketStatus::Revoked);
    assert_eq!(client.get_ticket(&t2).status, TicketStatus::Revoked);
}

#[test]
fn verify_tickets_returns_all_tickets() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let t1 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let t2 = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "2"),
        &2_000i128,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(t1);
    batch.push_back(t2);

    let results = client.verify_tickets(&batch);
    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap().original_price, 1_000);
    assert_eq!(results.get(1).unwrap().original_price, 2_000);
}

#[test]
fn check_in_requires_the_organizers_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    // Replace the blanket auth mock with one that authorizes only a stranger.
    // The organizer address matches the event, but the organizer has not
    // signed, so `require_auth` must fail before any state change.
    let stranger = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "check_in",
            args: (&organizer, ticket_id).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_check_in(&organizer, &ticket_id);
    // A host auth failure, not a contract Error such as NotOrganizer.
    assert!(matches!(result, Err(Err(_))));
    assert_eq!(client.verify_ticket(&ticket_id).status, TicketStatus::Valid);

    // With the organizer's own auth the same call succeeds, and the
    // recorded authorization belongs to the organizer.
    env.mock_auths(&[MockAuth {
        address: &organizer,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "check_in",
            args: (&organizer, ticket_id).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.check_in(&organizer, &ticket_id);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, organizer);
    assert_eq!(client.verify_ticket(&ticket_id).status, TicketStatus::Used);
}

/// Current behaviour: unlike `revoke_with_refund` (which rejects used
/// tickets), `revoke_ticket` has no status guard, so an organizer may revoke
/// a ticket that was already checked in and it becomes permanently Revoked.
#[test]
fn revoke_ticket_on_a_used_ticket_succeeds_and_marks_it_revoked() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    client.check_in(&organizer, &ticket_id);
    assert_eq!(client.verify_ticket(&ticket_id).status, TicketStatus::Used);

    client.revoke_ticket(&organizer, &ticket_id);
    assert_eq!(
        client.verify_ticket(&ticket_id).status,
        TicketStatus::Revoked
    );

    // The ticket now reports Revoked rather than AlreadyUsed on re-entry.
    let result = client.try_check_in(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::Revoked)));
}

#[test]
fn revoke_ticket_clears_the_resale_listing_data() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // cap is 120% of face value
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );

    client.list_for_resale(&buyer, &ticket_id, &1_200i128);
    let listed = client.get_ticket(&ticket_id);
    assert_eq!(listed.status, TicketStatus::Resale);
    assert_eq!(listed.resale_price, 1_200);

    client.revoke_ticket(&organizer, &ticket_id);

    // #129 — the asking price must not survive on a revoked ticket.
    let revoked = client.get_ticket(&ticket_id);
    assert_eq!(revoked.status, TicketStatus::Revoked);
    assert_eq!(revoked.resale_price, 0);
}

#[test]
fn revoke_ticket_rejects_already_revoked_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    client.revoke_ticket(&organizer, &ticket_id);
    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Revoked);

    let result = client.try_revoke_ticket(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::Revoked)));
}

#[test]
fn revoke_with_refund_rejects_already_revoked_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    client.revoke_ticket(&organizer, &ticket_id);
    let result = client.try_revoke_with_refund(&organizer, &ticket_id, &false);
    assert_eq!(result, Err(Ok(Error::Revoked)));
}

#[test]
fn revoke_batch_rejects_already_revoked_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    client.revoke_ticket(&organizer, &ticket_id);
    let mut batch = Vec::new(&env);
    batch.push_back(ticket_id);

    let result = client.try_revoke_batch(&organizer, &batch);
    assert_eq!(result, Err(Ok(Error::Revoked)));
}
