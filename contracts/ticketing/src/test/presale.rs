//! Tests for presale functionality and allowlist enforcement (Issues #451, #452).

use super::*;

fn make_event_with_presale(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    event_id: u64,
    presale_starts_at: Option<u64>,
    presale_ends_at: Option<u64>,
) {
    client.create_event(
        organizer,
        &event_id,
        &EventParams {
            name: String::from_str(env, "Presale Event"),
            category: String::from_str(env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at,
            presale_ends_at,
        },
    );
}

#[test]
fn presale_with_only_start_time_is_rejected() {
    // Issue #452: Reject events where exactly one presale bound is set.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Bad Presale Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: Some(5_000),
            presale_ends_at: None,
        },
    );

    assert_eq!(result, Err(Ok(Error::InvalidEventTime)));
}

#[test]
fn presale_with_only_end_time_is_rejected() {
    // Issue #452: Reject events where exactly one presale bound is set.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_create_event(
        &organizer,
        &1,
        &EventParams {
            name: String::from_str(&env, "Bad Presale Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 500,
            starts_at: 10_000,
            transfer_freeze_seconds: 100,
            resale_cutoff_seconds: 200,
            max_per_wallet: None,
            refund_window_seconds: None,
            presale_starts_at: None,
            presale_ends_at: Some(8_000),
        },
    );

    assert_eq!(result, Err(Ok(Error::InvalidEventTime)));
}

#[test]
fn presale_with_both_bounds_is_accepted() {
    // Issue #452: Both bounds must be Some, or both must be None.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    make_event_with_presale(&env, &client, &organizer, 1, Some(5_000), Some(8_000));

    let event = client.get_event(&1);
    assert_eq!(event.presale_starts_at, Some(5_000));
    assert_eq!(event.presale_ends_at, Some(8_000));
}

#[test]
fn presale_with_no_bounds_is_accepted() {
    // Issue #452: Both bounds must be Some, or both must be None.
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    make_event_with_presale(&env, &client, &organizer, 1, None, None);

    let event = client.get_event(&1);
    assert_eq!(event.presale_starts_at, None);
    assert_eq!(event.presale_ends_at, None);
}

#[test]
fn non_allowlisted_buyer_cannot_purchase_before_presale_starts() {
    // Issue #451: Before presale_starts_at, in_presale is false and any wallet
    // can buy. This test ensures non-allowlisted buyers are rejected whenever
    // now < presale_ends.
    let (env, client, token, token_asset, _admin, organizer) = setup();

    // Create event with presale window: 5000 to 8000
    make_event_with_presale(&env, &client, &organizer, 1, Some(5_000), Some(8_000));

    // Set tier price
    let tier = String::from_str(&env, "GA");
    client.set_tier_price(&organizer, &1, &tier, &1_000);

    // Create a buyer who is not on the allowlist
    let non_allowlisted_buyer = Address::generate(&env);
    mint_tokens(&token_asset, &non_allowlisted_buyer, 10_000);

    // Set ledger timestamp before presale starts (e.g., 4000)
    env.ledger().with_mut(|li| {
        li.timestamp = 4_000;
    });

    // Attempt to purchase should fail
    let result = client.try_purchase_primary(
        &non_allowlisted_buyer,
        &1,
        &tier,
        &String::from_str(&env, "unassigned"),
    );

    assert_eq!(result, Err(Ok(Error::NotOnAllowlist)));
}

#[test]
fn allowlisted_buyer_can_purchase_before_presale_starts() {
    // Issue #451: Allowlisted buyers should be able to buy before presale
    // starts and during presale.
    let (env, client, token, token_asset, _admin, organizer) = setup();

    // Create event with presale window: 5000 to 8000
    make_event_with_presale(&env, &client, &organizer, 1, Some(5_000), Some(8_000));

    // Set tier price
    let tier = String::from_str(&env, "GA");
    client.set_tier_price(&organizer, &1, &tier, &1_000);

    // Create an allowlisted buyer
    let allowlisted_buyer = Address::generate(&env);
    client.add_to_allowlist(&organizer, &1, &allowlisted_buyer);
    mint_tokens(&token_asset, &allowlisted_buyer, 10_000);

    // Set ledger timestamp before presale starts (e.g., 4000)
    env.ledger().with_mut(|li| {
        li.timestamp = 4_000;
    });

    // Purchase should succeed
    let ticket_id = client.purchase_primary(
        &allowlisted_buyer,
        &1,
        &tier,
        &String::from_str(&env, "unassigned"),
    );

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, allowlisted_buyer);
    assert_eq!(token.balance(&organizer), 1_000);
}

#[test]
fn non_allowlisted_buyer_cannot_purchase_during_presale() {
    // Issue #451: During presale window, only allowlisted buyers can purchase.
    let (env, client, token, token_asset, _admin, organizer) = setup();

    // Create event with presale window: 5000 to 8000
    make_event_with_presale(&env, &client, &organizer, 1, Some(5_000), Some(8_000));

    // Set tier price
    let tier = String::from_str(&env, "GA");
    client.set_tier_price(&organizer, &1, &tier, &1_000);

    // Create a buyer who is not on the allowlist
    let non_allowlisted_buyer = Address::generate(&env);
    mint_tokens(&token_asset, &non_allowlisted_buyer, 10_000);

    // Set ledger timestamp during presale (e.g., 6000)
    env.ledger().with_mut(|li| {
        li.timestamp = 6_000;
    });

    // Attempt to purchase should fail
    let result = client.try_purchase_primary(
        &non_allowlisted_buyer,
        &1,
        &tier,
        &String::from_str(&env, "unassigned"),
    );

    assert_eq!(result, Err(Ok(Error::NotOnAllowlist)));
}

#[test]
fn allowlisted_buyer_can_purchase_during_presale() {
    // Allowlisted buyers should be able to buy during presale.
    let (env, client, token, token_asset, _admin, organizer) = setup();

    // Create event with presale window: 5000 to 8000
    make_event_with_presale(&env, &client, &organizer, 1, Some(5_000), Some(8_000));

    // Set tier price
    let tier = String::from_str(&env, "GA");
    client.set_tier_price(&organizer, &1, &tier, &1_000);

    // Create an allowlisted buyer
    let allowlisted_buyer = Address::generate(&env);
    client.add_to_allowlist(&organizer, &1, &allowlisted_buyer);
    mint_tokens(&token_asset, &allowlisted_buyer, 10_000);

    // Set ledger timestamp during presale (e.g., 6000)
    env.ledger().with_mut(|li| {
        li.timestamp = 6_000;
    });

    // Purchase should succeed
    let ticket_id = client.purchase_primary(
        &allowlisted_buyer,
        &1,
        &tier,
        &String::from_str(&env, "unassigned"),
    );

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, allowlisted_buyer);
    assert_eq!(token.balance(&organizer), 1_000);
}

#[test]
fn anyone_can_purchase_after_presale_ends() {
    // Issue #451: After presale ends (now >= presale_ends_at), anyone can buy.
    let (env, client, token, token_asset, _admin, organizer) = setup();

    // Create event with presale window: 5000 to 8000
    make_event_with_presale(&env, &client, &organizer, 1, Some(5_000), Some(8_000));

    // Set tier price
    let tier = String::from_str(&env, "GA");
    client.set_tier_price(&organizer, &1, &tier, &1_000);

    // Create a buyer who is not on the allowlist
    let non_allowlisted_buyer = Address::generate(&env);
    mint_tokens(&token_asset, &non_allowlisted_buyer, 10_000);

    // Set ledger timestamp after presale ends (e.g., 8000)
    env.ledger().with_mut(|li| {
        li.timestamp = 8_000;
    });

    // Purchase should succeed
    let ticket_id = client.purchase_primary(
        &non_allowlisted_buyer,
        &1,
        &tier,
        &String::from_str(&env, "unassigned"),
    );

    let ticket = client.get_ticket(&ticket_id);
    assert_eq!(ticket.owner, non_allowlisted_buyer);
    assert_eq!(token.balance(&organizer), 1_000);
}
