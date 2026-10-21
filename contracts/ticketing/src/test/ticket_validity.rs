//! Time-based ticket validity windows (issue #72).
//!
//! A ticket may carry its own `(valid_from, valid_until)` window, e.g. the
//! days a multi-day pass covers. Check-in is rejected outside that window.

use super::*;

const FROM: u64 = 2_000;
const UNTIL: u64 = 3_000;

fn ticket(env: &Env, client: &TicketingContractClient, organizer: &Address) -> u64 {
    issue_sample_ticket(env, client, organizer, 1, &Address::generate(env), 1_000)
}

#[test]
fn tickets_have_no_window_by_default() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = ticket(&env, &client, &organizer);

    assert_eq!(client.ticket_validity(&ticket_id), (None, None));
    client.check_in(&organizer, &ticket_id);
}

#[test]
fn check_in_is_limited_to_the_window_inclusive() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let early = ticket(&env, &client, &organizer);
    let at_from = ticket(&env, &client, &organizer);
    let at_until = ticket(&env, &client, &organizer);
    let late = ticket(&env, &client, &organizer);
    for id in [early, at_from, at_until, late] {
        client.set_ticket_validity(&organizer, &id, &Some(FROM), &Some(UNTIL));
    }
    assert_eq!(client.ticket_validity(&early), (Some(FROM), Some(UNTIL)));

    env.ledger().set_timestamp(FROM - 1);
    assert_eq!(
        client.try_check_in(&organizer, &early),
        Err(Ok(Error::TicketNotYetValid))
    );
    assert_eq!(client.get_ticket(&early).status, TicketStatus::Valid);

    env.ledger().set_timestamp(FROM);
    client.check_in(&organizer, &at_from);

    env.ledger().set_timestamp(UNTIL);
    client.check_in(&organizer, &at_until);

    env.ledger().set_timestamp(UNTIL + 1);
    assert_eq!(
        client.try_check_in(&organizer, &late),
        Err(Ok(Error::TicketValidityEnded))
    );
}

#[test]
fn open_ended_bounds_only_restrict_one_side() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let from_only = ticket(&env, &client, &organizer);
    let until_only = ticket(&env, &client, &organizer);
    client.set_ticket_validity(&organizer, &from_only, &Some(FROM), &None);
    client.set_ticket_validity(&organizer, &until_only, &None, &Some(UNTIL));

    env.ledger().set_timestamp(FROM - 1);
    assert_eq!(
        client.try_check_in(&organizer, &from_only),
        Err(Ok(Error::TicketNotYetValid))
    );
    client.check_in(&organizer, &until_only);

    env.ledger().set_timestamp(UNTIL + 1_000);
    client.check_in(&organizer, &from_only);
}

#[test]
fn multi_day_pass_enters_on_each_day_of_its_window() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let pass = ticket(&env, &client, &organizer);
    client.set_ticket_max_uses(&organizer, &pass, &3u32);
    client.set_ticket_validity(&organizer, &pass, &Some(FROM), &Some(UNTIL));

    env.ledger().set_timestamp(FROM);
    client.check_in(&organizer, &pass);
    env.ledger().set_timestamp(UNTIL);
    client.check_in(&organizer, &pass);
    env.ledger().set_timestamp(UNTIL + 1);
    assert_eq!(
        client.try_check_in(&organizer, &pass),
        Err(Ok(Error::TicketValidityEnded))
    );
}

#[test]
fn batch_check_in_respects_the_window() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let open = ticket(&env, &client, &organizer);
    let windowed = ticket(&env, &client, &organizer);
    client.set_ticket_validity(&organizer, &windowed, &Some(FROM), &None);

    let mut ids = Vec::new(&env);
    ids.push_back(open);
    ids.push_back(windowed);
    env.ledger().set_timestamp(FROM - 1);
    assert_eq!(
        client.try_check_in_batch(&organizer, &ids),
        Err(Ok(Error::TicketNotYetValid))
    );
    assert_eq!(client.get_ticket(&open).status, TicketStatus::Valid);
}

#[test]
fn clearing_both_bounds_removes_the_window() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = ticket(&env, &client, &organizer);
    client.set_ticket_validity(&organizer, &ticket_id, &Some(FROM), &Some(UNTIL));
    client.set_ticket_validity(&organizer, &ticket_id, &None, &None);

    assert_eq!(client.ticket_validity(&ticket_id), (None, None));
    env.ledger().set_timestamp(FROM - 1);
    client.check_in(&organizer, &ticket_id);
}

#[test]
fn window_is_validated_and_organizer_only() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = ticket(&env, &client, &organizer);

    assert_eq!(
        client.try_set_ticket_validity(&organizer, &ticket_id, &Some(UNTIL), &Some(UNTIL)),
        Err(Ok(Error::InvalidValidityWindow))
    );
    assert_eq!(
        client.try_set_ticket_validity(&organizer, &ticket_id, &Some(UNTIL), &Some(FROM)),
        Err(Ok(Error::InvalidValidityWindow))
    );
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_ticket_validity(&stranger, &ticket_id, &Some(FROM), &None),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_set_ticket_validity(&organizer, &999, &Some(FROM), &None),
        Err(Ok(Error::TicketNotFound))
    );
}
