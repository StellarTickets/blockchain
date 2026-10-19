//! Admin-only WASM upgrade (issue #101).

use super::*;
use soroban_sdk::{Bytes, BytesN};

/// A minimal second build: a module with no imports or exports that carries
/// only the `contractenvmetav0` section the host requires. Its layout is
/// section id 0, size 30, a 17-byte name, then the XDR
/// `ScEnvMetaKind::InterfaceVersion` entry (kind 0, protocol 28, pre-release
/// 0), which matches the host's interface version. Calling the contract after
/// the upgrade fails because the new build exports no entry points.
const SECOND_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic and version
    0x00, 0x1e, 0x11, // custom section, 30 bytes, 17-byte name
    b'c', b'o', b'n', b't', b'r', b'a', b'c', b't', b'e', b'n', b'v', b'm', b'e', b't', b'a', b'v',
    b'0', // "contractenvmetav0"
    0x00, 0x00, 0x00, 0x00, // ScEnvMetaKind::InterfaceVersion
    0x00, 0x00, 0x00, 0x1c, // protocol 28
    0x00, 0x00, 0x00, 0x00, // pre-release 0
];

/// Uploads [`SECOND_WASM`] and returns its hash, ready for `upgrade`.
pub fn upload_second_wasm(env: &Env) -> BytesN<32> {
    env.deployer()
        .upload_contract_wasm(Bytes::from_slice(env, SECOND_WASM))
}

#[test]
fn admin_upgrades_to_a_second_wasm() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_hash = upload_second_wasm(&env);

    client.upgrade(&admin, &new_hash);

    // The first build serves `version`. The second build exports nothing, so
    // the same call now fails, which shows the contract code was replaced.
    assert!(client.try_version().is_err());
}

#[test]
fn non_admin_cannot_upgrade() {
    let (env, client, _token, _token_asset, _admin, _organizer) = setup();
    let intruder = Address::generate(&env);
    let new_hash = upload_second_wasm(&env);

    assert_eq!(
        client.try_upgrade(&intruder, &new_hash),
        Err(Ok(Error::NotAdmin))
    );
    // The rejected call left the first build in place.
    assert!(client.try_version().is_ok());
}
