//! Per-event custom metadata map (issue #70).

use super::*;

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

#[test]
fn events_have_no_metadata_by_default() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert!(client.event_metadata(&1).is_empty());
}

#[test]
fn organizer_sets_overwrites_and_removes_entries() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_metadata(&organizer, &1, &s(&env, "venue"), &s(&env, "O2 Arena"));
    client.set_event_metadata(&organizer, &1, &s(&env, "dress_code"), &s(&env, "smart"));

    let metadata = client.event_metadata(&1);
    assert_eq!(metadata.len(), 2);
    assert_eq!(metadata.get(s(&env, "venue")), Some(s(&env, "O2 Arena")));

    client.set_event_metadata(&organizer, &1, &s(&env, "venue"), &s(&env, "Wembley"));
    let metadata = client.event_metadata(&1);
    assert_eq!(metadata.len(), 2);
    assert_eq!(metadata.get(s(&env, "venue")), Some(s(&env, "Wembley")));

    client.remove_event_metadata(&organizer, &1, &s(&env, "venue"));
    client.remove_event_metadata(&organizer, &1, &s(&env, "missing"));
    let metadata = client.event_metadata(&1);
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata.get(s(&env, "venue")), None);

    client.remove_event_metadata(&organizer, &1, &s(&env, "dress_code"));
    assert!(client.event_metadata(&1).is_empty());
}

#[test]
fn metadata_map_is_bounded() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    // Keys "k", "kk", "kkk", ... are distinct and stay within the key budget.
    for i in 0..MAX_EVENT_METADATA_ENTRIES {
        let key = "k".repeat(i as usize + 1);
        client.set_event_metadata(&organizer, &1, &s(&env, &key), &s(&env, "v"));
    }

    assert_eq!(
        client.try_set_event_metadata(&organizer, &1, &s(&env, "one_more"), &s(&env, "v")),
        Err(Ok(Error::EventMetadataFull))
    );
    // Overwriting an existing key is still allowed once the map is full.
    client.set_event_metadata(&organizer, &1, &s(&env, "k"), &s(&env, "updated"));
    assert_eq!(client.event_metadata(&1).len(), MAX_EVENT_METADATA_ENTRIES);

    // Removing an entry frees a slot.
    client.remove_event_metadata(&organizer, &1, &s(&env, "kk"));
    client.set_event_metadata(&organizer, &1, &s(&env, "one_more"), &s(&env, "v"));
}

#[test]
fn keys_and_values_are_validated() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let long_key = "k".repeat(MAX_EVENT_METADATA_KEY_LEN as usize + 1);
    let long_value = "v".repeat(MAX_EVENT_METADATA_VALUE_LEN as usize + 1);

    for (key, value) in [
        ("", "v"),
        ("k", ""),
        (long_key.as_str(), "v"),
        ("k", long_value.as_str()),
    ] {
        assert_eq!(
            client.try_set_event_metadata(&organizer, &1, &s(&env, key), &s(&env, value)),
            Err(Ok(Error::InvalidEventMetadata))
        );
    }
    assert!(client.event_metadata(&1).is_empty());
}

#[test]
fn only_the_organizer_can_change_metadata() {
    let (env, client, _token, _asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_event_metadata(&organizer, &1, &s(&env, "venue"), &s(&env, "O2"));
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_set_event_metadata(&stranger, &1, &s(&env, "venue"), &s(&env, "x")),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_remove_event_metadata(&stranger, &1, &s(&env, "venue")),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_set_event_metadata(&organizer, &99, &s(&env, "venue"), &s(&env, "x")),
        Err(Ok(Error::EventNotFound))
    );
}
