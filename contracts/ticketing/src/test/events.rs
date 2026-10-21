use super::*;

#[test]
fn get_event_reports_not_found_for_an_unknown_id() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let result = client.try_get_event(&999);
    assert_eq!(result, Err(Ok(Error::EventNotFound)));
}

#[test]
fn create_event_rejects_a_duplicate_event_id() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let duplicate = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Other Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 10_000,
            royalty_bps: 0,
            starts_at: 10_000,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(duplicate, Err(Ok(Error::EventAlreadyExists)));
}

#[test]
fn get_organizer_events_returns_the_registered_event_count() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    assert_eq!(client.get_organizer_events(&organizer), 0);
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &organizer, 2);
    assert_eq!(client.get_organizer_events(&organizer), 2);
    let other = Address::generate(&env);
    assert_eq!(client.get_organizer_events(&other), 0);
}

#[test]
fn public_getters_return_typed_values_and_typed_errors() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &0i128,
    );
    assert_eq!(client.get_event(&1).tickets_issued, 1);
    assert_eq!(client.get_ticket(&ticket_id).owner, owner);
    assert_eq!(client.try_get_event(&999), Err(Ok(Error::EventNotFound)));
    assert_eq!(client.try_get_ticket(&999), Err(Ok(Error::TicketNotFound)));
}

#[test]
fn create_event_rejects_a_royalty_above_10_000_bps() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let bad_royalty = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 10_001, // >100%
            starts_at: 10_000,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(bad_royalty, Err(Ok(Error::InvalidRoyalty)));
}

#[test]
fn create_event_rejects_past_or_current_timestamp() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    env.ledger().set_timestamp(1_000);

    let in_the_past = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Past Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 10_000,
            royalty_bps: 0,
            starts_at: 999,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(in_the_past, Err(Ok(Error::InvalidEventTime)));

    let right_now = client.try_create_event(
        &organizer,
        &2,
        &EventParams {
            name: String::from_str(&env, "Now Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 10_000,
            royalty_bps: 0,
            starts_at: 1_000,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(right_now, Err(Ok(Error::InvalidEventTime)));
}

#[test]
fn organizer_can_run_multiple_independent_events() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &organizer, 2);

    let buyer = Address::generate(&env);
    client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    let ev1 = client.get_event(&1);
    let ev2 = client.get_event(&2);
    assert_eq!(ev1.tickets_issued, 1);
    assert_eq!(ev2.tickets_issued, 0);
}

#[test]
fn events_with_identical_names_have_independent_state() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let event_name = String::from_str(&env, "Summer Music Festival 2026");
    let category = String::from_str(&env, "concert");

    client.create_event(
        &organizer,
        &101u64,
        &EventParams {
            name: event_name.clone(),
            category: category.clone(),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );

    client.create_event(
        &organizer,
        &102u64,
        &EventParams {
            name: event_name,
            category,
            max_resale_multiplier_bps: 15_000,
            royalty_bps: 1_000,
            starts_at: 20_000,
            transfer_freeze_seconds: 150,
            resale_cutoff_seconds: 300,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );

    let event1 = client.get_event(&101u64);
    let event2 = client.get_event(&102u64);

    assert_eq!(event1.name, event2.name);
    assert_eq!(event1.category, event2.category);
    assert_ne!(event1.starts_at, event2.starts_at);
    assert_ne!(
        event1.max_resale_multiplier_bps,
        event2.max_resale_multiplier_bps
    );
    assert_ne!(event1.royalty_bps, event2.royalty_bps);

    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);

    let ticket1 = client.issue_ticket(
        &organizer,
        &101u64,
        &buyer1,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "Row 1"),
        &10_000i128,
    );

    let ticket2 = client.issue_ticket(
        &organizer,
        &102u64,
        &buyer2,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "Standing"),
        &5_000i128,
    );

    assert_eq!(client.get_event(&101u64).tickets_issued, 1);
    assert_eq!(client.get_event(&102u64).tickets_issued, 1);

    client.check_in(&organizer, &ticket1);
    assert_eq!(client.get_ticket(&ticket1).status, TicketStatus::Used);
    assert_eq!(client.get_ticket(&ticket2).status, TicketStatus::Valid);
}

#[test]
fn events_and_tickets_with_unicode_names_and_categories() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let unicode_name = String::from_str(&env, "東京ライブ 2026 🎵 (Tokyo Live)");
    let unicode_category = String::from_str(&env, "音楽 / Festival ✨");

    client.create_event(
        &organizer,
        &200u64,
        &EventParams {
            name: unicode_name.clone(),
            category: unicode_category.clone(),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );

    let event = client.get_event(&200u64);
    assert_eq!(event.name, unicode_name);
    assert_eq!(event.category, unicode_category);

    let buyer = Address::generate(&env);
    let unicode_tier = String::from_str(&env, "特別席 🌟 VIP");
    let unicode_seat = String::from_str(&env, "中央列-12番 🎟️");

    let ticket_id = client.issue_ticket(
        &organizer,
        &200u64,
        &buyer,
        &unicode_tier,
        &unicode_seat,
        &8_888i128,
    );

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.tier, unicode_tier);
    assert_eq!(ticket.seat, unicode_seat);
    assert_eq!(ticket.owner, buyer);
    assert_eq!(ticket.status, TicketStatus::Valid);
}

#[test]
fn lottery_allocates_requested_number_of_tickets() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let e1 = Address::generate(&env);
    let e2 = Address::generate(&env);
    let e3 = Address::generate(&env);
    let mut entrants = Vec::new(&env);
    entrants.push_back(e1);
    entrants.push_back(e2);
    entrants.push_back(e3);

    let winners = client.allocate_lottery(
        &organizer,
        &1,
        &entrants,
        &2u32,
        &String::from_str(&env, "VIP"),
        &0i128,
    );
    assert_eq!(winners.len(), 2);
    assert_eq!(client.get_event(&1).tickets_issued, 2);
}

#[test]
fn lottery_rejects_more_winners_than_entrants() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let mut entrants = Vec::new(&env);
    entrants.push_back(Address::generate(&env));

    let result = client.try_allocate_lottery(
        &organizer,
        &1,
        &entrants,
        &2u32,
        &String::from_str(&env, "VIP"),
        &0i128,
    );
    assert_eq!(result, Err(Ok(Error::InvalidLottery)));
}

#[test]
fn lottery_rejects_negative_price() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let mut entrants = Vec::new(&env);
    entrants.push_back(Address::generate(&env));

    let result = client.try_allocate_lottery(
        &organizer,
        &1,
        &entrants,
        &1u32,
        &String::from_str(&env, "VIP"),
        &-1i128,
    );
    assert_eq!(result, Err(Ok(Error::InvalidLottery)));
}

#[test]
fn lottery_rejects_duplicate_entrants() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let dup = Address::generate(&env);
    let mut entrants = Vec::new(&env);
    entrants.push_back(dup.clone());
    entrants.push_back(dup);

    let result = client.try_allocate_lottery(
        &organizer,
        &1,
        &entrants,
        &1u32,
        &String::from_str(&env, "VIP"),
        &0i128,
    );
    assert_eq!(result, Err(Ok(Error::InvalidLottery)));
}

#[test]
fn escrowed_primary_sale_holds_funds_in_the_contract() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);

    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    let event = client.get_event(&1);
    assert_eq!(event.escrow_balance, 2_000);
    assert_eq!(token.balance(&organizer), 0);
    assert_eq!(token.balance(&client.address), 2_000);
}

#[test]
fn release_escrow_rejects_before_the_event_ends() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);

    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    env.ledger().set_sequence_number(499);
    let result = client.try_release_escrow(&organizer, &1);
    assert_eq!(result, Err(Ok(Error::EventNotEnded)));
}

#[test]
fn release_escrow_pays_the_organizer_after_the_event_ends() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);

    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    env.ledger().set_sequence_number(500);
    client.release_escrow(&organizer, &1);

    assert_eq!(token.balance(&organizer), 2_000);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.get_event(&1).escrow_balance, 0);
}

#[test]
fn release_escrow_pays_out_in_per_event_payment_token() {
    let (env, client, token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Second token: the event's accepted payment token (issue #332).
    let token2_admin = Address::generate(&env);
    let token2_contract = env.register_stellar_asset_contract_v2(token2_admin);
    let token2 = TokenClient::new(&env, &token2_contract.address());
    let token2_asset = StellarAssetClient::new(&env, &token2_contract.address());

    client.set_event_payment_token(&organizer, &1, &Some(token2_contract.address()));
    assert_eq!(client.event_payment_token(&1), token2_contract.address());
    client.enable_escrow(&organizer, &1, &500u32);

    let buyer = Address::generate(&env);
    token2_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    // Escrow must be held in the event token, not the contract token.
    assert_eq!(client.get_event(&1).escrow_balance, 2_000);
    assert_eq!(token2.balance(&client.address), 2_000);
    assert_eq!(token2.balance(&buyer), 8_000);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(token.balance(&organizer), 0);

    env.ledger().set_sequence_number(500);
    client.release_escrow(&organizer, &1);

    // Release must pay out in the event token; contract-token balances stay put.
    assert_eq!(token2.balance(&organizer), 2_000);
    assert_eq!(token2.balance(&client.address), 0);
    assert_eq!(token.balance(&organizer), 0);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.get_event(&1).escrow_balance, 0);
}

#[test]
fn release_escrow_rejects_a_non_escrow_event() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let result = client.try_release_escrow(&organizer, &1);
    assert_eq!(result, Err(Ok(Error::EscrowNotEnabled)));
}

#[test]
fn enable_escrow_rejects_once_tickets_have_been_sold() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );
    let result = client.try_enable_escrow(&organizer, &1, &500u32);
    assert_eq!(result, Err(Ok(Error::EventAlreadyStarted)));
}

/// Issue #336: `enable_escrow` with a past (or current) release ledger is
/// rejected.
///
/// The escrow release ledger must be strictly in the future
/// (`escrow_release_ledger > current ledger`, see `enable_escrow` in
/// `lib.rs` which returns `Error::EventNotEnded` otherwise). A past ledger
/// would otherwise make `release_escrow` immediately releasable and defeat
/// the escrow hold.
#[test]
fn enable_escrow_rejects_past_release_ledger() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Pin the current ledger so the test does not depend on the default
    // sequence of the test host.
    env.ledger().set_sequence_number(500);

    // A release ledger in the past must be rejected.
    let past = client.try_enable_escrow(&organizer, &1, &499u32);
    assert_eq!(past, Err(Ok(Error::EventNotEnded)));

    // The boundary is strict: a release ledger equal to the current ledger
    // is not in the future and must also be rejected.
    let current = client.try_enable_escrow(&organizer, &1, &500u32);
    assert_eq!(current, Err(Ok(Error::EventNotEnded)));

    // Rejected calls must not partially enable escrow.
    let event = client.get_event(&1);
    assert!(
        !event.escrow_enabled,
        "rejected enable_escrow must leave escrow disabled"
    );
    assert_eq!(
        event.escrow_release_ledger, 0,
        "rejected enable_escrow must not set a release ledger"
    );

    // A strictly future ledger is accepted, proving the rejection above is
    // about the ledger and not some other event state.
    client.enable_escrow(&organizer, &1, &501u32);
    let event = client.get_event(&1);
    assert!(event.escrow_enabled);
    assert_eq!(event.escrow_release_ledger, 501);
}

#[test]
fn per_event_payment_token_routes_settlement() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let token2_admin = Address::generate(&env);
    let token2_contract = env.register_stellar_asset_contract_v2(token2_admin);
    let token2 = TokenClient::new(&env, &token2_contract.address());
    let token2_asset = StellarAssetClient::new(&env, &token2_contract.address());

    client.set_event_payment_token(&organizer, &1, &Some(token2_contract.address()));
    assert_eq!(client.event_payment_token(&1), token2_contract.address());

    let buyer = Address::generate(&env);
    token2_asset.mint(&buyer, &5_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    assert_eq!(token2.balance(&organizer), 2_000);
    assert_eq!(token2.balance(&buyer), 3_000);
}

#[test]
fn per_event_payment_token_rejects_change_after_tickets_issued() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    let buyer = Address::generate(&env);
    client.issue_ticket(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
        &1_000i128,
    );

    let token2_admin = Address::generate(&env);
    let token2_contract = env.register_stellar_asset_contract_v2(token2_admin);
    let err = client.try_set_event_payment_token(&organizer, &1, &Some(token2_contract.address()));
    assert_eq!(err, Err(Ok(Error::TicketsAlreadyIssued)));
}

#[test]
fn create_event_accepts_a_royalty_of_exactly_10_000_bps() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    client.create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 10_000, // exactly 100%: upper boundary is inclusive
            starts_at: 10_000,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(client.get_event(&1).royalty_bps, 10_000);
}

/// Issue #124: a zero multiplier is no longer accepted. Previously it was
/// stored and then silently made every resale impossible (cap 0), which is a
/// dead-end event rather than a valid configuration. The contract now rejects
/// it at creation with `Error::InvalidMultiplier`; see
/// `resale_multiplier.rs` for the full boundary coverage.
#[test]
fn create_event_rejects_zero_max_resale_multiplier() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 0,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: None,
        },
    );
    assert_eq!(result, Err(Ok(Error::InvalidMultiplier)));
}

/// #142: `purchase_primary` on an event id that was never created must
/// surface `Error::EventNotFound` rather than panicking or falling through
/// to a price/tier lookup on default-initialized state.
#[test]
fn purchase_primary_returns_event_not_found_for_a_nonexistent_event() {
    let (env, client, _token, token_asset, _admin, _organizer) = setup();
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    let result = client.try_purchase_primary(
        &buyer,
        &999,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );
    assert_eq!(result, Err(Ok(Error::EventNotFound)));
}

/// Issue #373: `release_escrow` resets `escrow_balance` to zero before
/// transferring funds. Calling it a second time must not pay out again — the
/// organizer balance must be unchanged after the second call.
#[test]
fn release_escrow_pays_out_only_once() {
    let (env, client, token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);

    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "1"),
    );

    // Advance past the escrow release ledger so `release_escrow` is allowed.
    env.ledger().set_sequence_number(500);

    // First release: escrow_balance (2_000) must be paid to organizer.
    client.release_escrow(&organizer, &1);
    let balance_after_first = token.balance(&organizer);
    assert_eq!(
        balance_after_first, 2_000,
        "first release_escrow must transfer the full escrowed amount"
    );
    assert_eq!(
        client.get_event(&1).escrow_balance,
        0,
        "escrow_balance must be zero after first release"
    );

    // Second release: escrow_balance is already 0, so the organizer must
    // receive nothing additional — the balance must be unchanged.
    client.release_escrow(&organizer, &1);
    let balance_after_second = token.balance(&organizer);
    assert_eq!(
        balance_after_second, balance_after_first,
        "second release_escrow must not pay out again"
    );
    assert_eq!(
        client.get_event(&1).escrow_balance,
        0,
        "escrow_balance must remain zero after second release"
    );
}
