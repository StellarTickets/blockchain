//! Per-owner ticket index and `tickets_of` pagination (issue #111).
//!
//! Each owner has a dense list of the ticket ids they hold. Minting, direct and
//! batch transfers, gift claims, resale purchases and burns keep that list in
//! step with the owner balance. The list is not chronological, because a
//! removal moves the owner's last entry into the freed slot.

use super::*;

fn holds(client: &TicketingContractClient, owner: &Address, ticket_id: u64) -> bool {
    client
        .tickets_of(owner, &0, &MAX_PAGE_SIZE)
        .iter()
        .any(|id| id == ticket_id)
}

#[test]
fn tickets_of_is_empty_for_an_address_with_no_tickets() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let stranger = Address::generate(&env);

    assert_eq!(client.tickets_of(&stranger, &0, &10).len(), 0);
}

#[test]
fn tickets_of_lists_every_issued_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    let first = issue_sample_ticket(&env, &client, &organizer, 1, &buyer, 1_000);
    let second = issue_sample_ticket(&env, &client, &organizer, 1, &buyer, 1_000);

    assert_eq!(client.tickets_of(&buyer, &0, &10).len(), 2);
    assert!(holds(&client, &buyer, first));
    assert!(holds(&client, &buyer, second));
}

#[test]
fn tickets_of_pages_through_the_list() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    for _ in 0..3 {
        issue_sample_ticket(&env, &client, &organizer, 1, &buyer, 1_000);
    }

    assert_eq!(client.tickets_of(&buyer, &0, &2).len(), 2);
    assert_eq!(client.tickets_of(&buyer, &2, &2).len(), 1);
    assert_eq!(client.tickets_of(&buyer, &5, &2).len(), 0);
}

#[test]
fn tickets_of_clamps_the_page_size_to_max_page_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    for _ in 0..=MAX_PAGE_SIZE {
        issue_sample_ticket(&env, &client, &organizer, 1, &buyer, 1_000);
    }

    assert_eq!(client.tickets_of(&buyer, &0, &1_000).len(), MAX_PAGE_SIZE);
}

#[test]
fn direct_transfer_moves_the_ticket_between_owner_lists() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.transfer_ticket(&owner, &ticket_id, &recipient);

    assert!(!holds(&client, &owner, ticket_id));
    assert!(holds(&client, &recipient, ticket_id));
    assert_eq!(client.tickets_of(&owner, &0, &10).len(), 0);
}

#[test]
fn transfer_keeps_the_remaining_tickets_listed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let first = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let second = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let third = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    // Removing the first entry moves the last one into its slot.
    client.transfer_ticket(&owner, &first, &recipient);
    assert_eq!(client.tickets_of(&owner, &0, &10).len(), 2);
    assert!(!holds(&client, &owner, first));
    assert!(holds(&client, &owner, second));
    assert!(holds(&client, &owner, third));

    client.transfer_ticket(&owner, &second, &recipient);
    assert_eq!(client.tickets_of(&owner, &0, &10).len(), 1);
    assert!(holds(&client, &owner, third));
    assert_eq!(client.tickets_of(&recipient, &0, &10).len(), 2);
    assert_eq!(client.balance(&owner), 1);
}

#[test]
fn batch_transfer_moves_every_ticket_in_the_batch() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let first = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let second = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let mut ticket_ids = Vec::new(&env);
    ticket_ids.push_back(first);
    ticket_ids.push_back(second);

    client.transfer_batch(&owner, &ticket_ids, &recipient);

    assert_eq!(client.tickets_of(&owner, &0, &10).len(), 0);
    assert!(holds(&client, &recipient, first));
    assert!(holds(&client, &recipient, second));
}

#[test]
fn resale_purchase_moves_the_ticket_to_the_buyer() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_000i128);
    token_asset.mint(&buyer, &10_000i128);

    client.buy_resale(&buyer, &ticket_id);

    assert!(!holds(&client, &seller, ticket_id));
    assert!(holds(&client, &buyer, ticket_id));
    assert_eq!(client.tickets_of(&seller, &0, &10).len(), 0);
}

#[test]
fn gift_claim_moves_the_ticket_to_the_recipient() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let secret = Bytes::from_slice(&env, b"gift-secret");
    let secret_hash: BytesN<32> = env.crypto().sha256(&secret).to_bytes();
    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64);

    client.claim_gift(&recipient, &ticket_id, &secret);

    assert!(!holds(&client, &owner, ticket_id));
    assert!(holds(&client, &recipient, ticket_id));
}

#[test]
fn burn_removes_the_ticket_from_the_owner_list() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.check_in(&organizer, &ticket_id);
    env.ledger()
        .set_timestamp(10_000 + USED_TICKET_RETENTION_SECONDS);

    client.burn_used_ticket(&organizer, &ticket_id);

    assert!(!holds(&client, &owner, ticket_id));
    assert_eq!(client.tickets_of(&owner, &0, &10).len(), 0);
    assert_eq!(client.balance(&owner), 0);
}

#[test]
fn pages_cover_each_remaining_ticket_once_after_a_burn_and_a_transfer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let t1 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let t2 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let t3 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let t4 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.transfer_ticket(&owner, &t1, &other);
    client.check_in(&organizer, &t2);
    env.ledger()
        .set_timestamp(10_000 + USED_TICKET_RETENTION_SECONDS);
    client.burn_used_ticket(&organizer, &t2);

    let first = client.tickets_of(&owner, &0, &1);
    let second = client.tickets_of(&owner, &1, &1);
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(first.get(0).unwrap(), second.get(0).unwrap());
    for id in [first.get(0).unwrap(), second.get(0).unwrap()] {
        assert!(id == t3 || id == t4);
    }
    assert_eq!(client.tickets_of(&owner, &2, &1).len(), 0);
    assert_eq!(client.tickets_of(&other, &0, &10).len(), 1);
}
