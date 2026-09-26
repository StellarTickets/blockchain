use super::*;

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

    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.original_price, 5_000);

    let event = client.get_event(&1);
    assert_eq!(event.tickets_issued, 1);
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
    let revoked_id =
        issue_sample_ticket(&env, &client, &organizer, 1, &revoked_owner, 1_000);

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
    let result = client.try_verify_ticket(&999);
    assert_eq!(result, Err(Ok(Error::TicketNotFound)));
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
    let ticket = client.verify_ticket(&ticket_id);
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

    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &2_000i128,
    );

    assert_eq!(token.balance(&organizer), 2_000);
    assert_eq!(token.balance(&buyer), 8_000);

    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.original_price, 2_000);
}

#[test]
fn purchase_primary_increments_tickets_issued() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    assert_eq!(client.get_event(&1).tickets_issued, 1);
}

#[test]
fn purchase_primary_allows_a_free_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);

    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "FREE"),
        &String::from_str(&env, "GA"),
        &0i128,
    );
    assert_eq!(client.verify_ticket(&ticket_id).original_price, 0);
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
    let ticket = client.verify_ticket(&ticket_id);
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
    let ticket = client.verify_ticket(&ticket_id);
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
    assert_eq!(client.verify_ticket(&ticket_id).owner, friend);

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
    assert_eq!(client.verify_ticket(&t1).owner, friend);
    assert_eq!(client.verify_ticket(&t2).owner, friend);
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

    let ticket = client.verify_ticket(&ticket_id);
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
    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Used);

    let result = client.try_check_in(&organizer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::AlreadyUsed)));
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
    assert_eq!(client.verify_ticket(&t1).status, TicketStatus::Used);
    assert_eq!(client.verify_ticket(&t2).status, TicketStatus::Used);
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

    let ticket_before = client.verify_ticket(&ticket_id);
    assert_eq!(ticket_before.owner, original_buyer);
    assert_eq!(ticket_before.status, TicketStatus::Valid);

    client.transfer_ticket(&original_buyer, &ticket_id, &new_owner);

    let ticket_transferred = client.verify_ticket(&ticket_id);
    assert_eq!(ticket_transferred.owner, new_owner);
    assert_eq!(ticket_transferred.status, TicketStatus::Valid);

    client.check_in(&organizer, &ticket_id);

    let ticket_checked_in = client.verify_ticket(&ticket_id);
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

    let ticket_id = client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &2_000i128,
    );

    assert_eq!(token.balance(&organizer), 2_000);
    assert_eq!(token.balance(&buyer), 8_000);

    token_asset.mint(&organizer, &5_000i128);
    client.revoke_with_refund(&organizer, &ticket_id, &true);

    assert_eq!(token.balance(&buyer), 10_000);
    assert_eq!(client.verify_ticket(&ticket_id).status, TicketStatus::Revoked);
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
    assert_eq!(client.verify_ticket(&t1).status, TicketStatus::Revoked);
    assert_eq!(client.verify_ticket(&t2).status, TicketStatus::Revoked);
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
