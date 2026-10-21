//! Paginated organizer event listing (Issue #112).
//!
//! `get_organizer_events` only counts an organizer's events. The paginated
//! `list_organizer_events` returns their ids a page at a time, so a frontend
//! can enumerate them without knowing the ids in advance.

use super::*;

fn ids(env: &Env, values: &[u64]) -> Vec<u64> {
    let mut out = Vec::new(env);
    for value in values {
        out.push_back(*value);
    }
    out
}

#[test]
fn lists_event_ids_in_creation_order() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 11);
    make_event(&env, &client, &organizer, 7);
    make_event(&env, &client, &organizer, 42);

    assert_eq!(
        client.list_organizer_events(&organizer, &0, &10),
        ids(&env, &[11, 7, 42])
    );
}

#[test]
fn pages_through_the_event_list() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &organizer, 2);
    make_event(&env, &client, &organizer, 3);

    assert_eq!(
        client.list_organizer_events(&organizer, &0, &2),
        ids(&env, &[1, 2])
    );
    assert_eq!(
        client.list_organizer_events(&organizer, &2, &2),
        ids(&env, &[3])
    );
    assert_eq!(
        client.list_organizer_events(&organizer, &3, &2),
        ids(&env, &[])
    );
}

#[test]
fn zero_limit_returns_an_empty_page() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_eq!(
        client.list_organizer_events(&organizer, &0, &0),
        ids(&env, &[])
    );
}

#[test]
fn start_past_the_end_does_not_overflow() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_eq!(
        client.list_organizer_events(&organizer, &u32::MAX, &10),
        ids(&env, &[])
    );
}

#[test]
fn page_size_is_clamped_to_max_page_size() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    for event_id in 1..=(MAX_PAGE_SIZE as u64 + 1) {
        make_event(&env, &client, &organizer, event_id);
    }

    let page = client.list_organizer_events(&organizer, &0, &u32::MAX);
    assert_eq!(page.len(), MAX_PAGE_SIZE);
    assert_eq!(page.get(0), Some(1));
}

#[test]
fn each_organizer_has_its_own_list() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    let other = Address::generate(&env);
    client.approve_organizer(&admin, &other);
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &other, 2);

    assert_eq!(
        client.list_organizer_events(&organizer, &0, &10),
        ids(&env, &[1])
    );
    assert_eq!(
        client.list_organizer_events(&other, &0, &10),
        ids(&env, &[2])
    );
}

#[test]
fn count_and_list_agree() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 5);
    make_event(&env, &client, &organizer, 6);

    assert_eq!(client.get_organizer_events(&organizer), 2);
    assert_eq!(
        client
            .list_organizer_events(&organizer, &0, &u32::MAX)
            .len(),
        2
    );
}
