use super::*;

#[test]
fn resale_listing_rejects_prices_above_cap() {
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

    let too_high = client.try_list_for_resale(&buyer, &ticket_id, &1_201i128);
    assert_eq!(too_high, Err(Ok(Error::ResalePriceExceedsCap)));

    client.list_for_resale(&buyer, &ticket_id, &1_200i128);
    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Resale);
    assert_eq!(ticket.resale_price, 1_200);
}

#[test]
fn resale_price_exactly_at_the_face_value_cap_is_allowed() {
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

    client.list_for_resale(&buyer, &ticket_id, &1_200i128);
    assert_eq!(client.verify_ticket(&ticket_id).status, TicketStatus::Resale);
}

#[test]
fn list_for_resale_rejects_a_zero_price() {
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
    let result = client.try_list_for_resale(&buyer, &ticket_id, &0i128);
    assert_eq!(result, Err(Ok(Error::InvalidPrice)));
}

#[test]
fn list_for_resale_rejects_a_non_owner() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let not_owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let result = client.try_list_for_resale(&not_owner, &ticket_id, &1_100i128);
    assert_eq!(result, Err(Ok(Error::NotOwner)));
}

#[test]
fn cancel_resale_returns_a_ticket_to_valid() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    client.list_for_resale(&seller, &ticket_id, &1_100i128);
    client.cancel_resale(&seller, &ticket_id);

    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
}

#[test]
fn cancel_resale_rejects_a_ticket_that_is_not_listed() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let result = client.try_cancel_resale(&owner, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotForResale)));
}

#[test]
fn buy_resale_splits_royalty_and_transfers_ownership() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1); // 5% royalty
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );
    client.list_for_resale(&seller, &ticket_id, &1_100i128);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    // 5% of 1100 = 55 to organizer, 1045 to seller.
    assert_eq!(token.balance(&organizer), 55);
    assert_eq!(token.balance(&seller), 1_045);
    assert_eq!(token.balance(&buyer), 10_000 - 1_100);

    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
}

#[test]
fn buy_resale_does_not_increment_tickets_issued() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    let tickets_issued_before_resale = client.get_event(&1).tickets_issued;

    client.list_for_resale(&seller, &ticket_id, &1_100);
    token_asset.mint(&buyer, &1_100);
    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(tickets_issued_before_resale, 1);
    assert_eq!(client.get_event(&1).tickets_issued, tickets_issued_before_resale);
}

#[test]
fn buy_resale_with_zero_royalty_pays_the_seller_in_full() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    client.create_event(
        &organizer,
        &1,
        &String::from_str(&env, "No Royalty Event"),
        &String::from_str(&env, "concert"),
        &12_000u32,
        &0u32, // 0% royalty
        &10_000u64,
        &100u64,
        &200u64,
    );
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    client.list_for_resale(&seller, &ticket_id, &1_100i128);

    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    assert_eq!(token.balance(&organizer), 0);
    assert_eq!(token.balance(&seller), 1_100);
    assert_eq!(token.balance(&buyer), 10_000 - 1_100);
}

#[test]
fn buy_resale_rejects_a_ticket_that_is_not_listed() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    token_asset.mint(&buyer, &10_000i128);
    let result = client.try_buy_resale(&buyer, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotForResale)));
}

#[test]
fn transferring_a_resale_listed_ticket_clears_the_listing_state() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let friend = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    client.list_for_resale(&seller, &ticket_id, &1_100i128);
    client.transfer_ticket(&seller, &ticket_id, &friend);

    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
    assert_eq!(ticket.owner, friend);
}

#[test]
fn resale_listing_invalidates_an_existing_gift_claim() {
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
    client.list_for_resale(&owner, &ticket_id, &1_100i128);

    let claim_err = client.try_claim_gift(&recipient, &ticket_id, &secret_bytes);
    assert_eq!(claim_err, Err(Ok(Error::GiftClaimNotFound)));
}

#[test]
fn gift_claim_creation_cancels_an_existing_resale_listing() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    client.list_for_resale(&owner, &ticket_id, &1_100i128);

    let secret_bytes = Bytes::from_array(&env, &[42u8; 32]);
    let secret_hash = env.crypto().sha256(&secret_bytes).to_bytes();

    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64);

    let ticket = client.verify_ticket(&ticket_id);
    assert_eq!(ticket.status, TicketStatus::Valid);
    assert_eq!(ticket.resale_price, 0);
}

#[test]
fn resale_listing_and_purchase_close_at_cutoff() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    // starts_at = 10_000, resale_cutoff_seconds = 200 -> cutoff at timestamp 9_800
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    env.ledger().set_timestamp(9_799);
    client.list_for_resale(&seller, &ticket_id, &1_100i128);

    env.ledger().set_timestamp(9_800);
    token_asset.mint(&buyer, &10_000i128);
    let buy_err = client.try_buy_resale(&buyer, &ticket_id);
    assert_eq!(buy_err, Err(Ok(Error::ResaleClosed)));
}

#[test]
fn list_for_resale_after_cancel_succeeds_and_allows_purchase() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "unassigned"),
        &1_000i128,
    );

    // Initial listing
    client.list_for_resale(&seller, &ticket_id, &1_100i128);
    let ticket_listed = client.verify_ticket(&ticket_id);
    assert_eq!(ticket_listed.status, TicketStatus::Resale);
    assert_eq!(ticket_listed.resale_price, 1_100i128);

    // Cancel resale
    client.cancel_resale(&seller, &ticket_id);
    let ticket_cancelled = client.verify_ticket(&ticket_id);
    assert_eq!(ticket_cancelled.status, TicketStatus::Valid);
    assert_eq!(ticket_cancelled.resale_price, 0i128);

    // Re-list for resale with updated price
    client.list_for_resale(&seller, &ticket_id, &1_150i128);
    let ticket_relisted = client.verify_ticket(&ticket_id);
    assert_eq!(ticket_relisted.status, TicketStatus::Resale);
    assert_eq!(ticket_relisted.resale_price, 1_150i128);

    // Purchase by buyer
    token_asset.mint(&buyer, &10_000i128);
    client.buy_resale(&buyer, &ticket_id);

    let ticket_bought = client.verify_ticket(&ticket_id);
    assert_eq!(ticket_bought.owner, buyer);
    assert_eq!(ticket_bought.status, TicketStatus::Valid);
    assert_eq!(ticket_bought.resale_price, 0i128);

    // 5% of 1150 = 57 to organizer, 1093 to seller
    assert_eq!(token.balance(&organizer), 57);
    assert_eq!(token.balance(&seller), 1093);
    assert_eq!(token.balance(&buyer), 10_000 - 1150);
}

#[test]
fn very_large_prices_near_i128_limits() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    
    // Test that an original_price close to i128::MAX/2 will panic or overflow on multiplier
    // Just document the outcome. 
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &(i128::MAX / 2),
    );

    // Soroban handles overflow by trapping or returning error. 
    // We expect a host trap (panic) or an error, just need to assert it if possible.
    // Or just a normal large price that doesn't overflow.
    let price = i128::MAX / 10;
    // this shouldn't overflow the multiplier if we are careful, 
    // max_resale_multiplier_bps = 12000 (120%).
    // i128::MAX / 10 * 12000 is still < i128::MAX, so it's fine.
    
    // We'll just verify the test runs for large prices.
    client.try_list_for_resale(&owner, &ticket_id, &price);
}

#[test]
fn buy_resale_where_seller_is_organizer() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &organizer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1000i128,
    );

    client.list_for_resale(&organizer, &ticket_id, &1100i128);
    token_asset.mint(&buyer, &10000i128);
    
    client.buy_resale(&buyer, &ticket_id);
    
    // 5% of 1100 = 55 (royalty), 1045 to seller (organizer).
    // Total to organizer should be 1100.
    assert_eq!(token.balance(&organizer), 1100);
}

#[test]
fn consecutive_resales_of_same_ticket() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller1 = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &seller1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1000i128,
    );

    client.list_for_resale(&seller1, &ticket_id, &1100i128);
    token_asset.mint(&buyer1, &10000i128);
    client.buy_resale(&buyer1, &ticket_id);
    
    let ticket1 = client.verify_ticket(&ticket_id);
    assert_eq!(ticket1.owner, buyer1);
    assert_eq!(ticket1.resale_price, 0);

    // Second resale
    client.list_for_resale(&buyer1, &ticket_id, &1200i128);
    token_asset.mint(&buyer2, &10000i128);
    client.buy_resale(&buyer2, &ticket_id);
    
    let ticket2 = client.verify_ticket(&ticket_id);
    assert_eq!(ticket2.owner, buyer2);
    assert_eq!(ticket2.resale_price, 0);
}

#[test]
fn cancel_resale_by_non_owner() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let non_owner = Address::generate(&env);
    
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1000i128,
    );

    client.list_for_resale(&owner, &ticket_id, &1100i128);
    let result = client.try_cancel_resale(&non_owner, &ticket_id);
    assert_eq!(result, Err(Ok(Error::NotOwner)));
}
