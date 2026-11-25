use super::*;

#[test]
fn organizer_checks_reject_another_events_organizer() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    let other = Address::generate(&env);
    client.approve_organizer(&admin, &other);
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &other, 2);
    let owner = Address::generate(&env);
    let id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let ids = soroban_sdk::vec![&env, id];
    let tier = String::from_str(&env, "GA");
    let seat = String::from_str(&env, "A1");
    let before = client.get_ticket(&id);

    assert_eq!(
        client.try_allocate_lottery(
            &other,
            &1,
            &soroban_sdk::vec![&env, owner.clone()],
            &1,
            &tier,
            &0
        ),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_enable_escrow(&other, &1, &500),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_set_event_payment_token(&other, &1, &None),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_issue_ticket(&other, &1, &owner, &tier, &seat, &0),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_set_tier_price(&other, &1, &tier, &0),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_release_escrow(&other, &1),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_check_in(&other, &id),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_check_in_batch(&other, &ids),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_revoke_ticket(&other, &id),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_set_seat(&other, &id, &seat),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_revoke_with_refund(&other, &id, &true),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(
        client.try_revoke_batch(&other, &ids),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(client.get_ticket(&id), before);
    assert_eq!(client.get_event(&1).tickets_issued, 1);
}

#[test]
fn organizer_check_preserves_missing_event_errors() {
    let (env, client, _token, _token_asset, _admin, organizer) = setup();
    let owner = Address::generate(&env);
    let tier = String::from_str(&env, "GA");
    assert_eq!(
        client.try_allocate_lottery(
            &organizer,
            &999,
            &soroban_sdk::vec![&env, owner.clone()],
            &1,
            &tier,
            &0
        ),
        Err(Ok(Error::EventNotFound))
    );
    assert_eq!(
        client.try_enable_escrow(&organizer, &999, &500),
        Err(Ok(Error::EventNotFound))
    );
    assert_eq!(
        client.try_set_event_payment_token(&organizer, &999, &None),
        Err(Ok(Error::EventNotFound))
    );
    assert_eq!(
        client.try_issue_ticket(&organizer, &999, &owner, &tier, &tier, &0),
        Err(Ok(Error::EventNotFound))
    );
    assert_eq!(
        client.try_set_tier_price(&organizer, &999, &tier, &0),
        Err(Ok(Error::EventNotFound))
    );
    assert_eq!(
        client.try_release_escrow(&organizer, &999),
        Err(Ok(Error::EventNotFound))
    );
}

#[test]
fn terminal_tickets_reject_transitions_without_changing_state_or_balances() {
    for expected in [Error::AlreadyUsed, Error::Revoked] {
        let (env, client, token, _token_asset, _admin, organizer) = setup();
        make_event(&env, &client, &organizer, 1);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);
        let id = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
        if expected == Error::AlreadyUsed {
            client.check_in(&organizer, &id);
        } else {
            client.revoke_ticket(&organizer, &id);
        }
        let before = client.get_ticket(&id);
        let ids = soroban_sdk::vec![&env, id];
        let secret = Bytes::from_slice(&env, b"gift-secret");
        let hash = env.crypto().sha256(&secret).to_bytes();

        assert_eq!(
            client.try_transfer_ticket(&owner, &id, &recipient),
            Err(Ok(expected))
        );
        assert_eq!(
            client.try_transfer_batch(&owner, &ids, &recipient),
            Err(Ok(expected))
        );
        assert_eq!(
            client.try_create_gift_claim(&owner, &id, &hash, &100),
            Err(Ok(expected))
        );
        assert_eq!(client.try_check_in(&organizer, &id), Err(Ok(expected)));
        assert_eq!(
            client.try_check_in_batch(&organizer, &ids),
            Err(Ok(expected))
        );
        assert_eq!(
            client.try_list_for_resale(&owner, &id, &1_100),
            Err(Ok(expected))
        );
        assert_eq!(
            client.try_revoke_with_refund(&organizer, &id, &true),
            Err(Ok(expected))
        );
        
        assert_eq!(client.get_ticket(&id), before);
        assert_eq!(token.balance(&owner), 0);
        assert_eq!(token.balance(&organizer), 0);
    }
}

#[test]
fn check_in_batch_rolls_back_when_a_later_ticket_belongs_to_another_organizer() {
    let (env, client, _token, _token_asset, admin, organizer) = setup();
    let other = Address::generate(&env);
    client.approve_organizer(&admin, &other);
    make_event(&env, &client, &organizer, 1);
    make_event(&env, &client, &other, 2);
    let owner = Address::generate(&env);
    let first = issue_sample_ticket(&env, &client, &organizer, 1, &owner, 1_000);
    let second = issue_sample_ticket(&env, &client, &other, 2, &owner, 1_000);
    let ids = soroban_sdk::vec![&env, first, second];

    assert_eq!(
        client.try_check_in_batch(&organizer, &ids),
        Err(Ok(Error::NotOrganizer))
    );
    assert_eq!(client.get_ticket(&first).status, TicketStatus::Valid);
    assert_eq!(client.get_ticket(&second).status, TicketStatus::Valid);
}
