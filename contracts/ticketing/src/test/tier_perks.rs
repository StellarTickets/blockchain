//! VIP/perks metadata per tier (issue #68).

use super::*;

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

fn perks(env: &Env, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new(env);
    for n in names {
        out.push_back(s(env, n));
    }
    out
}

#[test]
fn tiers_have_no_perks_by_default() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert!(client.tier_perks(&1, &s(&env, "VIP")).is_empty());
}

#[test]
fn organizer_sets_perks_per_tier() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_perks(
        &organizer,
        &1,
        &s(&env, "VIP"),
        &perks(&env, &["lounge access", "free drink"]),
    );

    assert_eq!(
        client.tier_perks(&1, &s(&env, "VIP")),
        perks(&env, &["lounge access", "free drink"])
    );
    assert!(client.tier_perks(&1, &s(&env, "GA")).is_empty());
    assert!(client.tier_perks(&2, &s(&env, "VIP")).is_empty());
}

#[test]
fn setting_perks_replaces_and_empty_list_removes_them() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_perks(&organizer, &1, &s(&env, "VIP"), &perks(&env, &["a", "b"]));
    client.set_tier_perks(&organizer, &1, &s(&env, "VIP"), &perks(&env, &["c"]));
    assert_eq!(client.tier_perks(&1, &s(&env, "VIP")), perks(&env, &["c"]));

    client.set_tier_perks(&organizer, &1, &s(&env, "VIP"), &Vec::new(&env));
    assert!(client.tier_perks(&1, &s(&env, "VIP")).is_empty());
}

#[test]
fn perk_lists_are_bounded_and_validated() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let mut many = Vec::new(&env);
    for _ in 0..=MAX_TIER_PERKS {
        many.push_back(s(&env, "perk"));
    }
    assert_eq!(
        client.try_set_tier_perks(&organizer, &1, &s(&env, "VIP"), &many),
        Err(Ok(Error::InvalidPerks))
    );
    assert_eq!(
        client.try_set_tier_perks(&organizer, &1, &s(&env, "VIP"), &perks(&env, &[""])),
        Err(Ok(Error::InvalidPerks))
    );
    let too_long = "x".repeat(MAX_PERK_LEN as usize + 1);
    assert_eq!(
        client.try_set_tier_perks(
            &organizer,
            &1,
            &s(&env, "VIP"),
            &perks(&env, &[too_long.as_str()])
        ),
        Err(Ok(Error::InvalidPerks))
    );
    let exact = "x".repeat(MAX_PERK_LEN as usize);
    client.set_tier_perks(
        &organizer,
        &1,
        &s(&env, "VIP"),
        &perks(&env, &[exact.as_str()]),
    );
}

#[test]
fn only_the_organizer_can_set_perks() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_set_tier_perks(&stranger, &1, &s(&env, "VIP"), &perks(&env, &["lounge"])),
        Err(Ok(Error::NotOrganizer))
    );
}
