//! Authorization matrix for every mutating entry point (Issue #136).
//!
//! Each test prepares the state a call needs under `mock_all_auths()`, then:
//!
//! 1. switches to `mock_auths(&[])` — no signatures at all — and asserts the
//!    call fails with a host auth error (`Err(Err(_))`), not a contract
//!    `Error`, so `require_auth()` is what rejected it;
//! 2. restores `mock_all_auths()`, repeats the identical call, and asserts it
//!    succeeds. Because the arguments are unchanged, this proves the first
//!    failure was the missing signature and not a business rule, and that
//!    the failed attempt left no state behind;
//! 3. asserts the recorded authorization belongs to the expected signer for
//!    that function, so dropping or retargeting a `require_auth()` call
//!    fails here.
//!
//! When adding a mutating entry point, add a row here.

use super::*;
use soroban_sdk::{
    testutils::{AuthorizedFunction, BytesN as _},
    BytesN, Symbol,
};

fn assert_signed_by(env: &Env, contract: &Address, signer: &Address, fn_name: &str) {
    let expected_fn = Symbol::new(env, fn_name);
    let signed = env.auths().iter().any(|(address, invocation)| {
        address == signer
            && matches!(
                &invocation.function,
                AuthorizedFunction::Contract((called, name, _))
                    if called == contract && *name == expected_fn
            )
    });
    assert!(
        signed,
        "`{fn_name}` must record an authorization from the expected signer"
    );
}

macro_rules! assert_requires_auth {
    ($env:expr, $client:expr, $signer:expr, $fn_name:literal, $call:expr) => {{
        $env.mock_auths(&[]);
        let denied = $call;
        assert!(
            matches!(denied, Err(Err(_))),
            "`{}` must fail with a host auth error when unsigned, got {:?}",
            $fn_name,
            denied
        );

        $env.mock_all_auths();
        let allowed = $call;
        assert!(
            allowed.is_ok(),
            "`{}` must succeed once signed, got {:?}",
            $fn_name,
            allowed
        );
        assert_signed_by(&$env, &$client.address, $signer, $fn_name);
    }};
}

fn s(env: &Env, value: &str) -> String {
    String::from_str(env, value)
}

fn one(env: &Env, id: u64) -> Vec<u64> {
    let mut ids = Vec::new(env);
    ids.push_back(id);
    ids
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[test]
fn initialize_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let client = TicketingContractClient::new(&env, &env.register(TicketingContract, ()));

    assert_requires_auth!(
        env,
        client,
        &admin,
        "initialize",
        client.try_initialize(&admin, &token)
    );
}

#[test]
fn propose_payment_token_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    assert_requires_auth!(
        env,
        client,
        &admin,
        "propose_payment_token",
        client.try_propose_payment_token(&admin, &new_token)
    );
}

#[test]
fn apply_payment_token_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    client.propose_payment_token(&admin, &new_token);
    env.ledger()
        .set_sequence_number(PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS + 1);

    assert_requires_auth!(
        env,
        client,
        &admin,
        "apply_payment_token",
        client.try_apply_payment_token(&admin)
    );
}

#[test]
fn set_purchase_throttle_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();

    assert_requires_auth!(
        env,
        client,
        &admin,
        "set_purchase_throttle",
        client.try_set_purchase_throttle(&admin, &10u32)
    );
}

#[test]
fn approve_organizer_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let newcomer = Address::generate(&env);

    assert_requires_auth!(
        env,
        client,
        &admin,
        "approve_organizer",
        client.try_approve_organizer(&admin, &newcomer)
    );
}

#[test]
fn revoke_organizer_requires_auth() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();

    assert_requires_auth!(
        env,
        client,
        &admin,
        "revoke_organizer",
        client.try_revoke_organizer(&admin, &organizer)
    );
}

// ---------------------------------------------------------------------------
// Organizer: events
// ---------------------------------------------------------------------------

#[test]
fn create_event_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "create_event",
        client.try_create_event(
            &organizer,
            &1,
            &EventParams {
                name: s(&env, "Show"),
                category: s(&env, "concert"),
                max_resale_multiplier_bps: 12_000,
                royalty_bps: 500,
                starts_at: 10_000,
                transfer_freeze_seconds: 100,
                resale_cutoff_seconds: 200,
            },
        )
    );
}

#[test]
fn create_event_with_options_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "create_event_with_options",
        client.try_create_event_with_options(
            &organizer,
            &1,
            &EventParams {
                name: s(&env, "Show"),
                category: s(&env, "concert"),
                max_resale_multiplier_bps: 12_000,
                royalty_bps: 500,
                starts_at: 10_000,
                transfer_freeze_seconds: 100,
                resale_cutoff_seconds: 200,
            },
            &Some(10_000u32),
            &Some(3u32),
        )
    );
}

#[test]
fn allocate_lottery_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let mut entrants = Vec::new(&env);
    entrants.push_back(Address::generate(&env));
    entrants.push_back(Address::generate(&env));

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "allocate_lottery",
        client.try_allocate_lottery(&organizer, &1, &entrants, &1u32, &s(&env, "VIP"), &0i128)
    );
}

#[test]
fn enable_escrow_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "enable_escrow",
        client.try_enable_escrow(&organizer, &1, &500u32)
    );
}

#[test]
fn set_event_payment_token_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let other_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "set_event_payment_token",
        client.try_set_event_payment_token(&organizer, &1, &Some(other_token.clone()))
    );
}

#[test]
fn set_tier_price_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "set_tier_price",
        client.try_set_tier_price(&organizer, &1, &s(&env, "GA"), &1_000i128)
    );
}

#[test]
fn release_escrow_requires_auth() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.enable_escrow(&organizer, &1, &500u32);
    client.set_tier_price(&organizer, &1, &s(&env, "GA"), &2_000i128);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);
    client.purchase_primary(&buyer, &1, &s(&env, "GA"), &s(&env, "1"));
    env.ledger().set_sequence_number(500);

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "release_escrow",
        client.try_release_escrow(&organizer, &1)
    );
}

// ---------------------------------------------------------------------------
// Organizer: tickets
// ---------------------------------------------------------------------------

#[test]
fn issue_ticket_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "issue_ticket",
        client.try_issue_ticket(
            &organizer,
            &1,
            &buyer,
            &s(&env, "GA"),
            &s(&env, "1"),
            &1_000i128
        )
    );
}

#[test]
fn check_in_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "check_in",
        client.try_check_in(&organizer, &ticket_id)
    );
}

#[test]
fn check_in_batch_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "check_in_batch",
        client.try_check_in_batch(&organizer, &one(&env, ticket_id))
    );
}

#[test]
fn revoke_ticket_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "revoke_ticket",
        client.try_revoke_ticket(&organizer, &ticket_id)
    );
}

#[test]
fn revoke_with_refund_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "revoke_with_refund",
        client.try_revoke_with_refund(&organizer, &ticket_id, &false)
    );
}

#[test]
fn revoke_batch_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "revoke_batch",
        client.try_revoke_batch(&organizer, &one(&env, ticket_id))
    );
}

#[test]
fn set_seat_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let ticket_id = issue_sample_ticket(
        &env,
        &client,
        &organizer,
        1,
        &Address::generate(&env),
        1_000,
    );

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "set_seat",
        client.try_set_seat(&organizer, &ticket_id, &s(&env, "Row 9"))
    );
}

// ---------------------------------------------------------------------------
// Buyers and ticket holders
// ---------------------------------------------------------------------------

#[test]
fn purchase_primary_requires_auth() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_tier_price(&organizer, &1, &s(&env, "GA"), &1_000i128);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);

    assert_requires_auth!(
        env,
        client,
        &buyer,
        "purchase_primary",
        client.try_purchase_primary(&buyer, &1, &s(&env, "GA"), &s(&env, "1"))
    );
}

#[test]
fn transfer_ticket_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_requires_auth!(
        env,
        client,
        &owner,
        "transfer_ticket",
        client.try_transfer_ticket(&owner, &ticket_id, &recipient)
    );
}

#[test]
fn transfer_batch_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_requires_auth!(
        env,
        client,
        &owner,
        "transfer_batch",
        client.try_transfer_batch(&owner, &one(&env, ticket_id), &recipient)
    );
}

#[test]
fn create_gift_claim_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let secret_hash = BytesN::<32>::random(&env);

    assert_requires_auth!(
        env,
        client,
        &owner,
        "create_gift_claim",
        client.try_create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64)
    );
}

#[test]
fn claim_gift_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let secret = Bytes::from_slice(&env, b"gift-secret");
    let secret_hash: BytesN<32> = env.crypto().sha256(&secret).to_bytes();
    client.create_gift_claim(&owner, &ticket_id, &secret_hash, &5_000u64);

    assert_requires_auth!(
        env,
        client,
        &recipient,
        "claim_gift",
        client.try_claim_gift(&recipient, &ticket_id, &secret)
    );
}

#[test]
fn list_for_resale_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);

    assert_requires_auth!(
        env,
        client,
        &owner,
        "list_for_resale",
        client.try_list_for_resale(&owner, &ticket_id, &1_100i128)
    );
}

#[test]
fn cancel_resale_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.list_for_resale(&owner, &ticket_id, &1_100i128);

    assert_requires_auth!(
        env,
        client,
        &owner,
        "cancel_resale",
        client.try_cancel_resale(&owner, &ticket_id)
    );
}

#[test]
fn buy_resale_requires_auth() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &seller, 1_000);
    client.list_for_resale(&seller, &ticket_id, &1_100i128);
    token_asset.mint(&buyer, &10_000i128);

    assert_requires_auth!(
        env,
        client,
        &buyer,
        "buy_resale",
        client.try_buy_resale(&buyer, &ticket_id)
    );
}

#[test]
fn burn_used_ticket_requires_auth() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let owner = Address::generate(&env);
    let ticket_id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    client.check_in(&organizer, &ticket_id);
    env.ledger()
        .set_timestamp(10_000 + USED_TICKET_RETENTION_SECONDS);

    assert_requires_auth!(
        env,
        client,
        &organizer,
        "burn_used_ticket",
        client.try_burn_used_ticket(&organizer, &ticket_id)
    );
}

#[test]
fn upgrade_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_hash = super::upgrade::upload_second_wasm(&env);

    assert_requires_auth!(
        env,
        client,
        &admin,
        "upgrade",
        client.try_upgrade(&admin, &new_hash)
    );
}

#[test]
fn propose_admin_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_admin = Address::generate(&env);

    assert_requires_auth!(
        env,
        client,
        &admin,
        "propose_admin",
        client.try_propose_admin(&admin, &new_admin)
    );
}

#[test]
fn accept_admin_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let new_admin = Address::generate(&env);
    client.propose_admin(&admin, &new_admin);

    assert_requires_auth!(
        env,
        client,
        &new_admin,
        "accept_admin",
        client.try_accept_admin(&new_admin)
    );
}

#[test]
fn set_platform_fee_requires_auth() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let platform = Address::generate(&env);

    assert_requires_auth!(
        env,
        client,
        &admin,
        "set_platform_fee",
        client.try_set_platform_fee(&admin, &250, &platform)
    );
}
