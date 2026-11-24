//! SEP-50 collection `name` and `symbol` (issue #505).
//!
//! The admin sets both once, and wallets read them back. Until then the
//! reads fail with `CollectionMetadataNotSet` instead of returning an empty
//! string, so a wallet can tell "unset" from "set to empty".

use super::*;

fn s(env: &Env, value: &str) -> String {
    String::from_str(env, value)
}

#[test]
fn name_and_symbol_are_unset_until_the_admin_sets_them() {
    let (_env, client, _token, _token_asset, _admin, _organizer) = setup();

    assert_eq!(client.try_name(), Err(Ok(Error::CollectionMetadataNotSet)));
    assert_eq!(
        client.try_symbol(),
        Err(Ok(Error::CollectionMetadataNotSet))
    );
}

#[test]
fn admin_sets_name_and_symbol_and_they_read_back() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    client.set_collection_metadata(&admin, &s(&env, "Stellar Tickets"), &s(&env, "TIX"));

    assert_eq!(client.name(), s(&env, "Stellar Tickets"));
    assert_eq!(client.symbol(), s(&env, "TIX"));
}

#[test]
fn setting_again_replaces_both_values() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    client.set_collection_metadata(&admin, &s(&env, "Old"), &s(&env, "OLD"));

    client.set_collection_metadata(&admin, &s(&env, "New Name"), &s(&env, "NEW"));

    assert_eq!(client.name(), s(&env, "New Name"));
    assert_eq!(client.symbol(), s(&env, "NEW"));
}

#[test]
fn only_the_admin_can_set_collection_metadata() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    assert_eq!(
        client.try_set_collection_metadata(&organizer, &s(&env, "Hijack"), &s(&env, "HJK")),
        Err(Ok(Error::NotAdmin))
    );
    assert_eq!(client.try_name(), Err(Ok(Error::CollectionMetadataNotSet)));
}

#[test]
fn empty_name_or_symbol_is_rejected() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    assert_eq!(
        client.try_set_collection_metadata(&admin, &s(&env, ""), &s(&env, "TIX")),
        Err(Ok(Error::InvalidCollectionMetadata))
    );
    assert_eq!(
        client.try_set_collection_metadata(&admin, &s(&env, "Tickets"), &s(&env, "")),
        Err(Ok(Error::InvalidCollectionMetadata))
    );
}

#[test]
fn length_limits_are_inclusive_and_enforced() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let max_name = "n".repeat(MAX_NAME_LEN as usize);
    let max_symbol = "S".repeat(MAX_COLLECTION_SYMBOL_LEN as usize);

    client.set_collection_metadata(&admin, &s(&env, &max_name), &s(&env, &max_symbol));
    assert_eq!(client.name(), s(&env, &max_name));
    assert_eq!(client.symbol(), s(&env, &max_symbol));

    let long_name = "n".repeat(MAX_NAME_LEN as usize + 1);
    assert_eq!(
        client.try_set_collection_metadata(&admin, &s(&env, &long_name), &s(&env, "TIX")),
        Err(Ok(Error::InvalidCollectionMetadata))
    );
    let long_symbol = "S".repeat(MAX_COLLECTION_SYMBOL_LEN as usize + 1);
    assert_eq!(
        client.try_set_collection_metadata(&admin, &s(&env, "Tickets"), &s(&env, &long_symbol)),
        Err(Ok(Error::InvalidCollectionMetadata))
    );
}

#[test]
fn rejected_update_keeps_the_previous_values() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    client.set_collection_metadata(&admin, &s(&env, "Keep"), &s(&env, "KEP"));

    let _ = client.try_set_collection_metadata(&admin, &s(&env, ""), &s(&env, "BAD"));

    assert_eq!(client.name(), s(&env, "Keep"));
    assert_eq!(client.symbol(), s(&env, "KEP"));
}
