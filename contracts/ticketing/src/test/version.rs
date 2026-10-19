//! Contract version read (issue #102).

use super::*;

#[test]
fn version_returns_the_crate_version() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();

    assert_eq!(
        client.version(),
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn version_is_readable_before_initialize() {
    let (env, client, _token, _admin, _organizer) = setup_uninitialized();

    assert_eq!(
        client.version(),
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    );
}
