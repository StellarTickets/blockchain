use super::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Property test for resale price cap math (issue #138, #140).
    ///
    /// Formula: `cap = original_price * max_resale_multiplier_bps / 10_000`
    /// Invariants:
    /// - `cap >= original_price` (since `multiplier_bps >= 10_000`)
    /// - Integer division truncates toward zero: `cap * 10_000 <= original_price * multiplier_bps`
    /// - `(cap + 1) * 10_000 > original_price * multiplier_bps`
    #[test]
    fn prop_resale_cap_math(
        original_price in 1i128..=1_000_000_000_000i128,
        multiplier_bps in 10_000u32..=50_000u32,
    ) {
        let cap = original_price * multiplier_bps as i128 / 10_000;
        let exact_product = original_price * multiplier_bps as i128;

        prop_assert!(cap >= original_price);
        prop_assert!(cap * 10_000 <= exact_product);
        prop_assert!((cap + 1) * 10_000 > exact_product);

        if exact_product % 10_000 != 0 {
            prop_assert!(cap * 10_000 < exact_product);
        } else {
            prop_assert_eq!(cap * 10_000, exact_product);
        }
    }

    /// Property test for organizer royalty split on resale (issue #138).
    ///
    /// Formula:
    /// `royalty = resale_price * royalty_bps / 10_000`
    /// `seller_amount = resale_price - royalty`
    /// Invariants:
    /// - Conservation of funds: `royalty + seller_amount == resale_price`
    /// - Non-negative splits: `royalty >= 0` and `seller_amount >= 0`
    /// - Bounded splits: `royalty <= resale_price` and `seller_amount <= resale_price`
    #[test]
    fn prop_royalty_split_math(
        resale_price in 1i128..=1_000_000_000_000i128,
        royalty_bps in 0u32..=10_000u32,
    ) {
        let royalty = resale_price * royalty_bps as i128 / 10_000;
        let seller_amount = resale_price - royalty;

        prop_assert_eq!(royalty + seller_amount, resale_price);
        prop_assert!(royalty >= 0);
        prop_assert!(seller_amount >= 0);
        prop_assert!(royalty <= resale_price);
        prop_assert!(seller_amount <= resale_price);

        if royalty_bps == 0 {
            prop_assert_eq!(royalty, 0);
            prop_assert_eq!(seller_amount, resale_price);
        } else if royalty_bps == 10_000 {
            prop_assert_eq!(royalty, resale_price);
            prop_assert_eq!(seller_amount, 0);
        }
    }

    /// Property test for floor and cap boundaries (issue #138).
    #[test]
    fn prop_resale_floor_and_cap_bounds(
        original_price in 100i128..=1_000_000_000i128,
        floor_bps in 1_000u32..=10_000u32,
        multiplier_bps in 10_000u32..=50_000u32,
    ) {
        let floor = original_price * floor_bps as i128 / 10_000;
        let cap = original_price * multiplier_bps as i128 / 10_000;

        prop_assert!(floor <= original_price);
        prop_assert!(cap >= original_price);
        prop_assert!(floor <= cap);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// On-chain proptest verifying price cap boundaries and settlement in the contract.
    #[test]
    fn prop_on_chain_resale_cap_and_settlement(
        original_price in 100i128..=10_000i128,
        multiplier_bps in 10_000u32..=25_000u32,
        royalty_bps in 0u32..=2_000u32,
    ) {
        let (env, client, token, token_asset, _admin, organizer) = setup();
        let event_id = 1u64;

        make_custom_event(
            &env,
            &client,
            &organizer,
            event_id,
            "PropTest Event",
            "concert",
            multiplier_bps,
            royalty_bps,
            10_000,
        );

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let ticket_id = client.issue_ticket(
            &organizer,
            &event_id,
            &seller,
            &String::from_str(&env, "GA"),
            &String::from_str(&env, "1"),
            &original_price,
        );

        let cap = original_price * multiplier_bps as i128 / 10_000;

        // Listing above cap must be rejected
        let too_high = client.try_list_for_resale(&seller, &ticket_id, &(cap + 1));
        prop_assert_eq!(too_high, Err(Ok(Error::ResalePriceExceedsCap)));

        // Listing at cap must succeed
        client.list_for_resale(&seller, &ticket_id, &cap);
        let ticket = client.get_ticket(&ticket_id);
        prop_assert_eq!(ticket.status, TicketStatus::Resale);
        prop_assert_eq!(ticket.resale_price, cap);

        // Buyer purchases at resale
        token_asset.mint(&buyer, &cap);
        let organizer_bal_before = token.balance(&organizer);
        let seller_bal_before = token.balance(&seller);

        client.buy_resale(&buyer, &ticket_id);

        let expected_royalty = cap * royalty_bps as i128 / 10_000;
        let expected_seller_amount = cap - expected_royalty;

        prop_assert_eq!(
            token.balance(&organizer) - organizer_bal_before,
            expected_royalty
        );
        prop_assert_eq!(
            token.balance(&seller) - seller_bal_before,
            expected_seller_amount
        );

        let bought = client.get_ticket(&ticket_id);
        prop_assert_eq!(bought.status, TicketStatus::Valid);
        prop_assert_eq!(bought.owner, buyer);
        prop_assert_eq!(bought.resale_price, 0);
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Transfer,
    ListResale,
    CancelResale,
    BuyResale,
    CheckIn,
    Revoke,
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        Just(Action::Transfer),
        Just(Action::ListResale),
        Just(Action::CancelResale),
        Just(Action::BuyResale),
        Just(Action::CheckIn),
        Just(Action::Revoke),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(25))]

    /// Property test on ticket state transitions and invariants across arbitrary action sequences (issue #138).
    ///
    /// Verifies state-machine invariants:
    /// - A ticket in `Resale` cannot be checked in (`Error::ResaleListingActive` - issue #132).
    /// - An already revoked ticket cannot be revoked again (`Error::Revoked` - issue #130).
    /// - Once `Used`, a ticket can never be transferred, listed for resale, or checked in again.
    /// - Once `Revoked`, a ticket cannot be transferred, listed for resale, checked in, or revoked again.
    /// - `resale_price > 0` if and only if `status == TicketStatus::Resale`.
    #[test]
    fn prop_ticket_state_transitions(
        actions in prop::collection::vec(action_strategy(), 3..=10),
    ) {
        let (env, client, _token, token_asset, _admin, organizer) = setup();
        let event_id = 100u64;

        make_custom_event(
            &env,
            &client,
            &organizer,
            event_id,
            "State Transition Event",
            "concert",
            12_000,
            500,
            10_000,
        );

        let mut current_owner = Address::generate(&env);
        let original_price = 1_000i128;
        let resale_price = 1_100i128;

        let ticket_id = client.issue_ticket(
            &organizer,
            &event_id,
            &current_owner,
            &String::from_str(&env, "GA"),
            &String::from_str(&env, "1"),
            &original_price,
        );

        for action in actions {
            let ticket_before = client.get_ticket(&ticket_id);

            match action {
                Action::Transfer => {
                    let next_owner = Address::generate(&env);
                    let res = client.try_transfer_ticket(&current_owner, &ticket_id, &next_owner);
                    match ticket_before.status {
                        TicketStatus::Valid | TicketStatus::Resale => {
                            prop_assert!(res.is_ok(), "Transfer should succeed from Valid or Resale");
                            current_owner = next_owner;
                            let ticket_after = client.get_ticket(&ticket_id);
                            prop_assert_eq!(ticket_after.status, TicketStatus::Valid);
                            prop_assert_eq!(ticket_after.owner, current_owner.clone());
                            prop_assert_eq!(ticket_after.resale_price, 0);
                        }
                        TicketStatus::Used => {
                            prop_assert_eq!(res, Err(Ok(Error::AlreadyUsed)));
                        }
                        TicketStatus::Revoked => {
                            prop_assert_eq!(res, Err(Ok(Error::Revoked)));
                        }
                    }
                }
                Action::ListResale => {
                    let res = client.try_list_for_resale(&current_owner, &ticket_id, &resale_price);
                    match ticket_before.status {
                        TicketStatus::Valid | TicketStatus::Resale => {
                            prop_assert!(res.is_ok(), "Listing should succeed from Valid or Resale");
                            let ticket_after = client.get_ticket(&ticket_id);
                            prop_assert_eq!(ticket_after.status, TicketStatus::Resale);
                            prop_assert_eq!(ticket_after.resale_price, resale_price);
                        }
                        TicketStatus::Used => {
                            prop_assert_eq!(res, Err(Ok(Error::AlreadyUsed)));
                        }
                        TicketStatus::Revoked => {
                            prop_assert_eq!(res, Err(Ok(Error::Revoked)));
                        }
                    }
                }
                Action::CancelResale => {
                    let res = client.try_cancel_resale(&current_owner, &ticket_id);
                    match ticket_before.status {
                        TicketStatus::Resale => {
                            prop_assert!(res.is_ok(), "Cancel resale should succeed from Resale");
                            let ticket_after = client.get_ticket(&ticket_id);
                            prop_assert_eq!(ticket_after.status, TicketStatus::Valid);
                            prop_assert_eq!(ticket_after.resale_price, 0);
                        }
                        _ => {
                            prop_assert_eq!(res, Err(Ok(Error::NotForResale)));
                        }
                    }
                }
                Action::BuyResale => {
                    let buyer = Address::generate(&env);
                    token_asset.mint(&buyer, &resale_price);
                    let res = client.try_buy_resale(&buyer, &ticket_id);
                    match ticket_before.status {
                        TicketStatus::Resale => {
                            prop_assert!(res.is_ok(), "Buy resale should succeed from Resale");
                            current_owner = buyer;
                            let ticket_after = client.get_ticket(&ticket_id);
                            prop_assert_eq!(ticket_after.status, TicketStatus::Valid);
                            prop_assert_eq!(ticket_after.owner, current_owner.clone());
                            prop_assert_eq!(ticket_after.resale_price, 0);
                        }
                        _ => {
                            prop_assert_eq!(res, Err(Ok(Error::NotForResale)));
                        }
                    }
                }
                Action::CheckIn => {
                    let res = client.try_check_in(&organizer, &ticket_id);
                    match ticket_before.status {
                        TicketStatus::Valid => {
                            prop_assert!(res.is_ok(), "Check-in should succeed from Valid");
                            let ticket_after = client.get_ticket(&ticket_id);
                            prop_assert_eq!(ticket_after.status, TicketStatus::Used);
                            prop_assert_eq!(ticket_after.resale_price, 0);
                        }
                        TicketStatus::Resale => {
                            // Issue #132: check_in rejected for Resale status
                            prop_assert_eq!(res, Err(Ok(Error::ResaleListingActive)));
                        }
                        TicketStatus::Used => {
                            prop_assert_eq!(res, Err(Ok(Error::AlreadyUsed)));
                        }
                        TicketStatus::Revoked => {
                            prop_assert_eq!(res, Err(Ok(Error::Revoked)));
                        }
                    }
                }
                Action::Revoke => {
                    let res = client.try_revoke_ticket(&organizer, &ticket_id);
                    match ticket_before.status {
                        TicketStatus::Valid | TicketStatus::Resale | TicketStatus::Used => {
                            prop_assert!(res.is_ok(), "Revocation should succeed from unrevoked status");
                            let ticket_after = client.get_ticket(&ticket_id);
                            prop_assert_eq!(ticket_after.status, TicketStatus::Revoked);
                            prop_assert_eq!(ticket_after.resale_price, 0);
                        }
                        TicketStatus::Revoked => {
                            // Issue #130: cannot revoke an already revoked ticket
                            prop_assert_eq!(res, Err(Ok(Error::Revoked)));
                        }
                    }
                }
            }

            // Universal invariants check after each step
            let current = client.get_ticket(&ticket_id);
            if current.status == TicketStatus::Resale {
                prop_assert!(current.resale_price > 0);
            } else {
                prop_assert_eq!(current.resale_price, 0);
            }
        }
    }
}
