use super::*;

fn make_event_with_options(
    env: &Env,
    client: &TicketingContractClient,
    organizer: &Address,
    event_id: u64,
    min_resale_multiplier_bps: Option<u32>,
    max_transfers_per_ticket: Option<u32>,
) {
    client.create_event_with_options(
        organizer,
        &event_id,
        &EventParams {
            name: String::from_str(env, "Controlled Event"),
            category: String::from_str(env, "concert"),
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
        &min_resale_multiplier_bps,
        &max_transfers_per_ticket,
    );
}

#[test]
fn resale_price_floor_is_enforced_when_configured() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event_with_options(&env, &client, &organizer, 1, Some(11_000), None);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    assert_eq!(
        client.try_list_for_resale(&owner, &ticket_id, &1_099i128),
        Err(Ok(Error::ResalePriceBelowFloor))
    );
    client.list_for_resale(&owner, &ticket_id, &1_100i128);
}

#[test]
fn transfer_limit_counts_direct_transfers() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event_with_options(&env, &client, &organizer, 1, None, Some(1));
    let owner = Address::generate(&env);
    let friend = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "GA"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    client.transfer_ticket(&owner, &ticket_id, &friend);
    assert_eq!(client.get_ticket(&ticket_id).transfers, 1);
    assert_eq!(
        client.try_transfer_ticket(&friend, &ticket_id, &organizer),
        Err(Ok(Error::TransferLimitExceeded))
    );
}

#[test]
fn organizer_can_reassign_a_ticket_seat() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "A1"),
        &1_000i128,
    );

    client.set_seat(&organizer, &ticket_id, &String::from_str(&env, "B4"));
    assert_eq!(
        client.get_ticket(&ticket_id).seat,
        String::from_str(&env, "B4")
    );
}
