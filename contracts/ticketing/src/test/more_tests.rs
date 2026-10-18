use super::*;

#[test]
fn allocate_lottery_rejects_non_organizer() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // Build a simple entrants list
    let mut entrants = Vec::new(&env);
    entrants.push_back(Address::generate(&env));

    // Call as a different (non-organizer) address
    let other = Address::generate(&env);
    let result = client.try_allocate_lottery(
        &other,
        &1,
        &entrants,
        &1u32,
        &String::from_str(&env, "VIP"),
        &0i128,
    );

    // Expect NotOrganizer error when a non-organizer attempts this action
    assert_eq!(result, Err(Ok(Error::NotOrganizer)));
}

#[test]
fn create_event_accepts_royalty_of_zero() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    client.create_event(
        &organizer,
        &10u64,
        &EventParams {
            name: String::from_str(&env, "Zero Royalty Event"),
            category: String::from_str(&env, "concert"),
            max_resale_multiplier_bps: 12_000,
            royalty_bps: 0, // boundary value
            starts_at: 10_000,
            transfer_freeze_seconds: 0,
            resale_cutoff_seconds: 0,
        },
    );

    // Confirm the event was stored with royalty 0
    assert_eq!(client.get_event(&10u64).royalty_bps, 0);
}

#[test]
fn set_event_payment_token_none_restores_contract_token() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    // register a second token and set it for the event
    let token2_admin = Address::generate(&env);
    let token2_contract = env.register_stellar_asset_contract_v2(token2_admin);

    client.set_event_payment_token(&organizer, &1, &Some(token2_contract.address()));
    assert_eq!(client.event_payment_token(&1), token2_contract.address());

    // Now reset the per-event token to `None` and expect the contract-wide token
    client.set_event_payment_token(&organizer, &1, &None);

    // The contract-wide payment token is the one set at initialization (admin-provided)
    let contract_token = client.payment_token();
    assert_eq!(client.event_payment_token(&1), contract_token);
}
