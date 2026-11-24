//! Zone / gate access restrictions per tier (issue #80).

use super::*;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::Event as _;

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

fn zones(env: &Env, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new(env);
    for n in names {
        out.push_back(s(env, n));
    }
    out
}

#[test]
fn restricted_tier_can_only_enter_its_zones() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let vip = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &s(&env, "VIP"),
        &s(&env, "A1"),
        &1_000,
    );
    let vip2 = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &s(&env, "VIP"),
        &s(&env, "A2"),
        &1_000,
    );
    client.set_tier_zones(
        &organizer,
        &1,
        &s(&env, "VIP"),
        &zones(&env, &["vip-lounge", "floor"]),
    );

    assert_eq!(
        client.try_check_in_at_zone(&organizer, &vip, &s(&env, "backstage")),
        Err(Ok(Error::ZoneNotAllowed))
    );
    assert_eq!(client.get_ticket(&vip).status, TicketStatus::Valid);

    client.check_in_at_zone(&organizer, &vip, &s(&env, "floor"));
    assert_eq!(client.get_ticket(&vip).status, TicketStatus::Used);
    client.check_in_at_zone(&organizer, &vip2, &s(&env, "vip-lounge"));
    assert_eq!(
        client.tier_zones(&1, &s(&env, "VIP")),
        zones(&env, &["vip-lounge", "floor"])
    );
}

#[test]
fn unrestricted_tier_enters_any_zone_and_plain_check_in_still_works() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ga = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let ga2 = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.set_tier_zones(
        &organizer,
        &1,
        &s(&env, "VIP"),
        &zones(&env, &["vip-lounge"]),
    );

    assert!(client.tier_zones(&1, &s(&env, "GA")).is_empty());
    client.check_in_at_zone(&organizer, &ga, &s(&env, "anywhere"));
    client.check_in(&organizer, &ga2);
    assert_eq!(client.get_ticket(&ga2).status, TicketStatus::Used);
}

#[test]
fn empty_zone_list_removes_the_restriction() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let vip = client.issue_ticket(
        &organizer,
        &1,
        &owner,
        &s(&env, "VIP"),
        &s(&env, "A1"),
        &1_000,
    );
    client.set_tier_zones(
        &organizer,
        &1,
        &s(&env, "VIP"),
        &zones(&env, &["vip-lounge"]),
    );
    client.set_tier_zones(&organizer, &1, &s(&env, "VIP"), &Vec::new(&env));

    client.check_in_at_zone(&organizer, &vip, &s(&env, "floor"));
}

#[test]
fn zone_lists_are_bounded_and_validated() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let mut many = Vec::new(&env);
    for _ in 0..=MAX_TIER_ZONES {
        many.push_back(s(&env, "z"));
    }
    assert_eq!(
        client.try_set_tier_zones(&organizer, &1, &s(&env, "VIP"), &many),
        Err(Ok(Error::InvalidZones))
    );
    assert_eq!(
        client.try_set_tier_zones(&organizer, &1, &s(&env, "VIP"), &zones(&env, &[""])),
        Err(Ok(Error::InvalidZones))
    );
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_tier_zones(&stranger, &1, &s(&env, "VIP"), &zones(&env, &["floor"])),
        Err(Ok(Error::NotOrganizer))
    );
}

/// Counts `TierZonesSet` events matching one exact zone change (issue #515).
fn count_tier_zones_set(
    env: &Env,
    client: &TicketingContractClient,
    zone_list: Vec<String>,
) -> usize {
    let expected = TierZonesSet {
        event_id: 1,
        tier: s(env, "VIP"),
        zones: zone_list,
    }
    .to_xdr(env, &client.address);
    env.events()
        .all()
        .events()
        .iter()
        .filter(|event| **event == expected)
        .count()
}

#[test]
fn set_tier_zones_publishes_the_zone_list_and_its_clearing() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    client.set_tier_zones(
        &organizer,
        &1,
        &s(&env, "VIP"),
        &zones(&env, &["vip-lounge", "floor"]),
    );
    assert_eq!(
        count_tier_zones_set(&env, &client, zones(&env, &["vip-lounge", "floor"])),
        1
    );

    client.set_tier_zones(&organizer, &1, &s(&env, "VIP"), &Vec::new(&env));
    assert_eq!(count_tier_zones_set(&env, &client, Vec::new(&env)), 1);
}

#[test]
fn rejected_tier_zones_publish_nothing() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_eq!(
        client.try_set_tier_zones(&organizer, &1, &s(&env, "VIP"), &zones(&env, &[""])),
        Err(Ok(Error::InvalidZones))
    );
    assert_eq!(count_tier_zones_set(&env, &client, zones(&env, &[""])), 0);
}
