//! Base URI fallback for `token_uri` (issue #506).
//!
//! `purchase_primary` mints without a per-ticket metadata URI, so its tickets
//! used to return `MetadataUriNotSet` and break NFT wallets. The admin can now
//! set a base URI, and `token_uri` returns `{base}/{token_id}` for any ticket
//! without its own URI. A per-ticket URI always wins.

use super::*;

const BASE: &str = "https://tickets.example.com/meta";
const TICKET_URI: &str = "ipfs://bafybeigdyrzt/ticket.json";

/// Expected `{BASE}/{id}` built without `format!`, since the crate is `no_std`.
fn expected_uri(env: &Env, id: u64) -> String {
    let mut buf = [0u8; 64];
    let base = BASE.as_bytes();
    buf[..base.len()].copy_from_slice(base);
    let mut n = base.len();
    buf[n] = b'/';
    n += 1;
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    let mut rest = id;
    loop {
        start -= 1;
        digits[start] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    let text = &digits[start..];
    buf[n..n + text.len()].copy_from_slice(text);
    n += text.len();
    String::from_bytes(env, &buf[..n])
}

fn free_tier(env: &Env, client: &TicketingContractClient, organizer: &Address) {
    client.set_tier_price(organizer, &1, &String::from_str(env, "GA"), &0i128);
}

fn buy_free_ticket(env: &Env, client: &TicketingContractClient) -> u64 {
    let buyer = Address::generate(env);
    client.purchase_primary(
        &buyer,
        &1,
        &String::from_str(env, "GA"),
        &String::from_str(env, "A1"),
    )
}

#[test]
fn primary_sale_ticket_falls_back_to_the_base_uri() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    free_tier(&env, &client, &organizer);
    client.set_base_token_uri(&admin, &Some(String::from_str(&env, BASE)));

    let ticket_id = buy_free_ticket(&env, &client);

    assert_eq!(client.token_uri(&ticket_id), expected_uri(&env, ticket_id));
}

#[test]
fn multi_digit_ids_are_appended_in_full() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    free_tier(&env, &client, &organizer);
    client.set_base_token_uri(&admin, &Some(String::from_str(&env, BASE)));

    let mut last = 0u64;
    for _ in 0..12 {
        last = buy_free_ticket(&env, &client);
    }

    assert!(last >= 10, "test needs a multi-digit id, got {last}");
    assert_eq!(client.token_uri(&last), expected_uri(&env, last));
}

#[test]
fn per_ticket_uri_takes_precedence_over_the_base_uri() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    client.set_base_token_uri(&admin, &Some(String::from_str(&env, BASE)));
    let buyer = Address::generate(&env);

    let ticket_id = client.issue_ticket_with_metadata(
        &organizer,
        &1,
        &buyer,
        &String::from_str(&env, "VIP"),
        &String::from_str(&env, "A12"),
        &1_000i128,
        &Some(String::from_str(&env, TICKET_URI)),
    );

    assert_eq!(
        client.token_uri(&ticket_id),
        String::from_str(&env, TICKET_URI)
    );
}

#[test]
fn without_a_base_uri_primary_tickets_still_report_no_uri() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    free_tier(&env, &client, &organizer);

    let ticket_id = buy_free_ticket(&env, &client);

    assert_eq!(
        client.try_token_uri(&ticket_id),
        Err(Ok(Error::MetadataUriNotSet))
    );
}

#[test]
fn clearing_the_base_uri_restores_the_missing_uri_error() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    free_tier(&env, &client, &organizer);
    client.set_base_token_uri(&admin, &Some(String::from_str(&env, BASE)));
    let ticket_id = buy_free_ticket(&env, &client);
    assert!(client.try_token_uri(&ticket_id).is_ok());

    client.set_base_token_uri(&admin, &None);

    assert_eq!(
        client.try_token_uri(&ticket_id),
        Err(Ok(Error::MetadataUriNotSet))
    );
}

#[test]
fn only_the_admin_can_set_the_base_uri() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();

    let result = client.try_set_base_token_uri(&organizer, &Some(String::from_str(&env, BASE)));

    assert_eq!(result, Err(Ok(Error::NotAdmin)));
}

#[test]
fn rejects_an_empty_base_uri_and_keeps_the_old_value() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    free_tier(&env, &client, &organizer);
    client.set_base_token_uri(&admin, &Some(String::from_str(&env, BASE)));

    let result = client.try_set_base_token_uri(&admin, &Some(String::from_str(&env, "")));

    assert_eq!(result, Err(Ok(Error::EmptyNameOrCategory)));
    let ticket_id = buy_free_ticket(&env, &client);
    assert_eq!(client.token_uri(&ticket_id), expected_uri(&env, ticket_id));
}

#[test]
fn rejects_a_base_uri_longer_than_the_metadata_limit() {
    let (env, client, _token, _token_asset, admin, _organizer) = setup();
    let too_long = String::from_bytes(&env, &[b'x'; 300]);

    let result = client.try_set_base_token_uri(&admin, &Some(too_long));

    assert_eq!(result, Err(Ok(Error::StringTooLong)));
}

#[test]
fn a_base_uri_that_overflows_with_the_id_is_rejected() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    free_tier(&env, &client, &organizer);
    // 255 bytes fits the base limit, but "/{id}" pushes the result past it.
    client.set_base_token_uri(&admin, &Some(String::from_bytes(&env, &[b'x'; 255])));
    let ticket_id = buy_free_ticket(&env, &client);

    assert_eq!(
        client.try_token_uri(&ticket_id),
        Err(Ok(Error::StringTooLong))
    );
}
