//! Event cancellation (issue #91).
//!
//! `cancel_event` marks an event cancelled. Once cancelled, the event rejects
//! new issuance, primary sales and resale listings. Tickets issued before the
//! cancel stay valid.

use super::*;

fn ga(env: &Env) -> String {
    String::from_str(env, "GA")
}

#[test]
fn cancel_event_sets_the_flag_and_is_organizer_only() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_cancel_event(&stranger, &1),
        Err(Ok(Error::NotOrganizer))
    );
    assert!(!client.get_event(&1).cancelled);

    client.cancel_event(&organizer, &1);
    assert!(client.get_event(&1).cancelled);
}

#[test]
fn cancelling_twice_is_rejected() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.cancel_event(&organizer, &1);

    assert_eq!(
        client.try_cancel_event(&organizer, &1),
        Err(Ok(Error::EventCancelled))
    );
}

#[test]
fn cancelled_event_blocks_issue_ticket() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.cancel_event(&organizer, &1);
    let owner = Address::generate(&env);

    assert_eq!(
        client.try_issue_ticket(
            &organizer,
            &1,
            &owner,
            &ga(&env),
            &String::from_str(&env, "unassigned"),
            &1_000i128,
        ),
        Err(Ok(Error::EventCancelled))
    );
    assert_eq!(client.get_event(&1).tickets_issued, 0);
}

#[test]
fn cancelled_event_blocks_purchase_primary() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_price(&organizer, &1, &ga(&env), &0i128);
    client.cancel_event(&organizer, &1);
    let buyer = Address::generate(&env);

    assert_eq!(
        client.try_purchase_primary(&buyer, &1, &ga(&env), &String::from_str(&env, "A1")),
        Err(Ok(Error::EventCancelled))
    );
}

#[test]
fn cancelled_event_blocks_listing_for_resale() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.cancel_event(&organizer, &1);

    assert_eq!(
        client.try_list_for_resale(&seller, &ticket_id, &1_100i128),
        Err(Ok(Error::EventCancelled))
    );
}

#[test]
fn cancelled_event_blocks_batch_and_lottery_minting() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.cancel_event(&organizer, &1);

    let mut recipients = Vec::new(&env);
    recipients.push_back(TicketRecipient {
        to: Address::generate(&env),
        tier: ga(&env),
        seat: String::from_str(&env, "unassigned"),
        price: 1_000,
        metadata_uri: None,
    });
    assert_eq!(
        client.try_issue_tickets(&organizer, &1, &recipients),
        Err(Ok(Error::EventCancelled))
    );

    let entrants = soroban_sdk::vec![&env, Address::generate(&env)];
    assert_eq!(
        client.try_allocate_lottery(&organizer, &1, &entrants, &1u32, &ga(&env), &0i128),
        Err(Ok(Error::EventCancelled))
    );
}

#[test]
fn tickets_issued_before_cancel_stay_valid() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    client.cancel_event(&organizer, &1);

    assert_eq!(client.get_ticket(&ticket_id).status, TicketStatus::Valid);
    assert!(client.is_valid(&ticket_id, &owner));
}
