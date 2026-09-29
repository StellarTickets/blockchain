//! StellarTickets ticketing smart contract for Soroban on Stellar.
#![no_std]
#![allow(missing_docs)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::pedantic)]

mod constants;
mod error;
mod events;
mod types;

pub use constants::{
    BPS_DENOMINATOR, CATEGORY_CONCERT, CATEGORY_CONFERENCE, CATEGORY_FESTIVAL, CATEGORY_FLIGHT,
    CATEGORY_OTHER, CATEGORY_SPORTS, MAX_BATCH_SIZE, MAX_CATEGORY_LEN, MAX_NAME_LEN,
    MAX_TICKET_LABEL_LEN, PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS,
};
pub use error::Error;
pub use events::{
    ContractInitialized, OrganizerApproved, OrganizerRevoked, PaymentTokenChanged,
    PaymentTokenProposed, PurchaseThrottleUpdated, TicketCheckedIn, TicketIssued,
};
pub use types::{
    Category, DataKey, Event, GiftClaim, PendingPaymentToken, StoredTicket, Ticket, TicketStatus,
};

use soroban_sdk::{contract, contractimpl, token, Address, Bytes, BytesN, Env, String, Vec};

const LEDGER_BUMP: u32 = 535_679; // ~31 days at 5s/ledger, matches other Soroban tooling defaults
const LEDGER_THRESHOLD: u32 = 500_000;

/// # Authorization ordering (issue #206)
///
/// Every state-changing entry point below calls `require_auth()` as its
/// very first statement, before any storage lookup or business-rule check
/// (existence of a ticket/event, ownership, status, etc.). This ordering is
/// deliberate: checking auth first means a caller who has not authorized
/// the call learns nothing about contract state from the error they get
/// back — not whether a ticket exists, who owns it, or what state it's in.
/// Validating business rules before auth would leak that information to an
/// unauthenticated caller through which error is returned. Keep this order
/// when adding new entry points. See
/// `require_auth_runs_before_business_validation` in `test/auth.rs` for the
/// regression test.
#[allow(missing_docs)]
#[contract]
pub struct TicketingContract;

#[allow(missing_docs)]
#[contractimpl]
impl TicketingContract {
    /// One-time setup. `payment_token` is the Stellar Asset Contract (or any
    /// SEP-41 token) used for on-chain primary sales and resale settlement.
    pub fn initialize(env: Env, admin: Address, payment_token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        // The probe validates that `payment_token` is a real token contract
        // and captures its decimals for client display (issue #233).
        let decimals = Self::ensure_token_contract(&env, &payment_token)?;
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PaymentToken, &payment_token);
        env.storage()
            .instance()
            .set(&DataKey::TokenDecimals, &decimals);
        env.storage().instance().set(&DataKey::NextTicketId, &0u64);
        env.storage()
            .instance()
            .extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP);
        ContractInitialized {
            admin,
            payment_token,
        }
        .publish(&env);
        Ok(())
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn require_initialized(env: &Env) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            Ok(())
        } else {
            Err(Error::NotInitialized)
        }
    }

    fn validate_label(label: &String, max_len: u32) -> Result<(), Error> {
        let len = label.len();
        if len == 0 {
            return Err(Error::EmptyNameOrCategory);
        }
        if len > max_len {
            return Err(Error::StringTooLong);
        }
        Ok(())
    }

    /// Event metadata validation: non-empty, length-bounded name and category.
    fn validate_event_labels(name: &String, category: &String) -> Result<(), Error> {
        Self::validate_label(name, MAX_NAME_LEN)?;
        Self::validate_label(category, MAX_CATEGORY_LEN)
    }

    /// Ticket label validation: non-empty, length-bounded tier and seat.
    fn validate_ticket_labels(tier: &String, seat: &String) -> Result<(), Error> {
        Self::validate_label(tier, MAX_TICKET_LABEL_LEN)?;
        Self::validate_label(seat, MAX_TICKET_LABEL_LEN)
    }

    /// Step one of a payment token change: the admin proposes a new token,
    /// which can only be applied after `PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS`.
    /// A new proposal replaces any pending one. Proceeds already held in
    /// escrow stay denominated in the old token, so release them first.
    pub fn propose_payment_token(
        env: Env,
        admin: Address,
        new_token: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        // The probe validates that the proposed token is a real token
        // contract. Its decimals are captured when the change is APPLIED so
        // clients always see decimals matching the active payment token
        // (issue #233).
        Self::ensure_token_contract(&env, &new_token)?;
        let apply_after_ledger = env
            .ledger()
            .sequence()
            .saturating_add(PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS);
        env.storage().instance().set(
            &DataKey::PendingPaymentToken,
            &PendingPaymentToken {
                token: new_token.clone(),
                apply_after_ledger,
            },
        );
        PaymentTokenProposed {
            admin,
            new_token,
            apply_after_ledger,
        }
        .publish(&env);
        Ok(())
    }

    /// Step two: once the delay has elapsed, the admin applies the pending
    /// payment token.
    pub fn apply_payment_token(env: Env, admin: Address) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        let pending: PendingPaymentToken = env
            .storage()
            .instance()
            .get(&DataKey::PendingPaymentToken)
            .ok_or(Error::NoPendingPaymentToken)?;
        if env.ledger().sequence() < pending.apply_after_ledger {
            return Err(Error::TimelockNotElapsed);
        }
        let old_token = Self::payment_token(&env)?;
        // Capture the decimals of the token becoming active so clients always
        // see decimals matching the active payment token (issue #233).
        let decimals = Self::ensure_token_contract(&env, &pending.token)?;
        env.storage()
            .instance()
            .set(&DataKey::PaymentToken, &pending.token);
        env.storage()
            .instance()
            .set(&DataKey::TokenDecimals, &decimals);
        env.storage()
            .instance()
            .remove(&DataKey::PendingPaymentToken);
        PaymentTokenChanged {
            admin,
            old_token,
            new_token: pending.token,
        }
        .publish(&env);
        Ok(())
    }

    /// Adds `organizer` to the allowlist of addresses permitted to create
    /// events (issue #135). Idempotent. See `docs/ORGANIZER_ALLOWLIST.md`.
    pub fn approve_organizer(env: Env, admin: Address, organizer: Address) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        let key = DataKey::ApprovedOrganizer(organizer.clone());
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        OrganizerApproved { admin, organizer }.publish(&env);
        Ok(())
    }

    /// Removes `organizer` from the event-creation allowlist (issue #135).
    /// Events the organizer already created are unaffected; only new
    /// `create_event*` calls are blocked. Idempotent.
    pub fn revoke_organizer(env: Env, admin: Address, organizer: Address) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .persistent()
            .remove(&DataKey::ApprovedOrganizer(organizer.clone()));
        OrganizerRevoked { admin, organizer }.publish(&env);
        Ok(())
    }

    /// Whether `organizer` is currently allowed to create events.
    pub fn is_approved_organizer(env: Env, organizer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::ApprovedOrganizer(organizer))
    }

    /// Registers a new event/route/showing under an organizer. `event_id` is
    /// chosen by the caller's backend (e.g. a ULID cast to u64) so it can be
    /// correlated with the off-chain event record. Only organizers on the
    /// admin-managed allowlist may call this, so an arbitrary address cannot
    /// squat on an id the backend is about to use (issue #135).
    pub fn create_event(
        env: Env,
        organizer: Address,
        event_id: u64,
        name: String,
        category: String,
        max_resale_multiplier_bps: u32,
        royalty_bps: u32,
        starts_at: u64,
        transfer_freeze_seconds: u64,
        resale_cutoff_seconds: u64,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_initialized(&env)?;
        Self::require_approved_organizer(&env, &organizer)?;
        if royalty_bps > 10_000 {
            return Err(Error::InvalidRoyalty);
        }
        // Issue #124: a multiplier below face value (10_000 bps) makes every
        // resale impossible — `list_for_resale` computes the cap as
        // `original_price * max_resale_multiplier_bps / 10_000`, which lands
        // below the face value and rejects any listing at or above it. Require
        // the multiplier to be at least face value.
        if max_resale_multiplier_bps < BPS_DENOMINATOR {
            return Err(Error::InvalidMultiplier);
        }
        // Issue #125: reject empty or oversized name/category.
        Self::validate_event_labels(&name, &category)?;
        if starts_at <= env.ledger().timestamp() {
            return Err(Error::InvalidEventTime);
        }
        let key = DataKey::Event(event_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::EventAlreadyExists);
        }
        let event = Event {
            organizer,
            name,
            category,
            max_resale_multiplier_bps,
            min_resale_multiplier_bps: None,
            max_transfers_per_ticket: None,
            royalty_bps,
            tickets_issued: 0,
            starts_at,
            transfer_freeze_seconds,
            resale_cutoff_seconds,
            escrow_enabled: false,
            escrow_release_ledger: 0,
            escrow_balance: 0,
            payment_token: None,
        };
        Self::save_event(&env, event_id, &event);
        env.storage()
            .persistent()
            .set(&DataKey::TicketsIssued(event_id), &0u64);
        env.storage().persistent().extend_ttl(
            &DataKey::TicketsIssued(event_id),
            LEDGER_THRESHOLD,
            LEDGER_BUMP,
        );
        Self::increment_organizer_events(&env, &event.organizer);
        Ok(())
    }

    /// Registers an event with optional resale floor and transfer limit.
    /// Kept separate from `create_event` to preserve the original entrypoint
    /// ABI for existing deployments.
    pub fn create_event_with_options(
        env: Env,
        organizer: Address,
        event_id: u64,
        name: String,
        category: String,
        max_resale_multiplier_bps: u32,
        royalty_bps: u32,
        starts_at: u64,
        transfer_freeze_seconds: u64,
        resale_cutoff_seconds: u64,
        min_resale_multiplier_bps: Option<u32>,
        max_transfers_per_ticket: Option<u32>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_initialized(&env)?;
        Self::require_approved_organizer(&env, &organizer)?;
        if royalty_bps > 10_000 {
            return Err(Error::InvalidRoyalty);
        }
        // Issue #124: a multiplier below face value (10_000 bps) makes every
        // resale impossible — `list_for_resale` computes the cap as
        // `original_price * max_resale_multiplier_bps / 10_000`, which lands
        // below the face value and rejects any listing at or above it. Require
        // the multiplier to be at least face value.
        if max_resale_multiplier_bps < BPS_DENOMINATOR {
            return Err(Error::InvalidMultiplier);
        }
        // Issue #125: reject empty or oversized name/category.
        Self::validate_event_labels(&name, &category)?;
        if starts_at <= env.ledger().timestamp() {
            return Err(Error::InvalidEventTime);
        }
        let key = DataKey::Event(event_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::EventAlreadyExists);
        }
        let event = Event {
            organizer,
            name,
            category,
            max_resale_multiplier_bps,
            min_resale_multiplier_bps,
            max_transfers_per_ticket,
            royalty_bps,
            tickets_issued: 0,
            starts_at,
            transfer_freeze_seconds,
            resale_cutoff_seconds,
            escrow_enabled: false,
            escrow_release_ledger: 0,
            escrow_balance: 0,
            payment_token: None,
        };
        Self::save_event(&env, event_id, &event);
        env.storage()
            .persistent()
            .set(&DataKey::TicketsIssued(event_id), &0u64);
        env.storage().persistent().extend_ttl(
            &DataKey::TicketsIssued(event_id),
            LEDGER_THRESHOLD,
            LEDGER_BUMP,
        );
        Self::increment_organizer_events(&env, &event.organizer);
        Ok(())
    }

    /// Randomly allocates complimentary/reserved tickets across a supplied
    /// entrant set. The organizer controls the entrant list; winner selection
    /// is performed on-chain using Soroban's PRNG.
    pub fn allocate_lottery(
        env: Env,
        organizer: Address,
        event_id: u64,
        mut entrants: Vec<Address>,
        winner_count: u32,
        tier: String,
        price: i128,
    ) -> Result<Vec<u64>, Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        if price < 0 || winner_count == 0 || winner_count > entrants.len() {
            return Err(Error::InvalidLottery);
        }
        for i in 0..entrants.len() {
            for j in (i + 1)..entrants.len() {
                if entrants.get(i) == entrants.get(j) {
                    return Err(Error::InvalidLottery);
                }
            }
        }
        let event = Self::get_event_inner(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }

        env.prng().shuffle(&mut entrants);
        let mut ticket_ids = Vec::new(&env);
        for i in 0..winner_count {
            let winner = entrants.get(i).unwrap();
            let ticket_id = Self::mint(
                &env,
                event_id,
                winner,
                tier.clone(),
                String::from_str(&env, "unassigned"),
                price,
            );
            ticket_ids.push_back(ticket_id);
        }

        Self::increment_tickets_issued(&env, event_id, winner_count as u64);
        Ok(ticket_ids)
    }

    /// Opts an event into escrow: primary sale proceeds are held by the
    /// contract instead of paid to the organizer immediately, and can only
    /// be released via `release_escrow` once the ledger sequence reaches
    /// `escrow_release_ledger` (e.g. the event's end). Must be called
    /// before any tickets are sold, since it would otherwise change the
    /// settlement terms for purchases already made.
    pub fn enable_escrow(
        env: Env,
        organizer: Address,
        event_id: u64,
        escrow_release_ledger: u32,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::get_event_inner(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        if event.tickets_issued > 0 {
            return Err(Error::EventAlreadyStarted);
        }
        event.escrow_enabled = true;
        event.escrow_release_ledger = escrow_release_ledger;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Sets the event's accepted payment token (issue #235). When set, the
    /// event's primary sales, resale settlement, escrow and refunds are all
    /// denominated in this token instead of the contract-wide payment token.
    /// Pass `None` to fall back to the contract-wide token. Only callable by
    /// the organizer while no tickets have been issued — existing sales must
    /// stay denominated in the token they were paid in.
    pub fn set_event_payment_token(
        env: Env,
        organizer: Address,
        event_id: u64,
        token: Option<Address>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::get_event_inner(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        if event.tickets_issued > 0 {
            return Err(Error::TicketsAlreadyIssued);
        }
        if let Some(token) = &token {
            Self::ensure_token_contract(&env, token)?;
        }
        event.payment_token = token;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Returns the event's accepted payment token: the per-event override
    /// when one is set, otherwise the contract-wide payment token
    /// (issue #235).
    pub fn event_payment_token(env: Env, event_id: u64) -> Result<Address, Error> {
        Self::extend_instance_ttl(&env);
        Self::require_initialized(&env)?;
        let event = Self::get_event_inner(&env, event_id)?;
        Self::payment_token_for_event(&env, &event)
    }

    /// Organizer-authorized issuance for tickets already paid for off-chain
    /// (card payment, comp, or fiat-to-crypto settled by the platform).
    pub fn issue_ticket(
        env: Env,
        organizer: Address,
        event_id: u64,
        to: Address,
        tier: String,
        seat: String,
        price: i128,
    ) -> Result<u64, Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_initialized(&env)?;
        if price < 0 {
            return Err(Error::InvalidPrice);
        }
        // Issue #126: bound tier/seat so they cannot inflate per-ticket
        // storage cost and rent.
        Self::validate_ticket_labels(&tier, &seat)?;
        let event = Self::get_event_inner(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        let ticket_id = Self::mint(&env, event_id, to, tier, seat, price);
        Self::increment_tickets_issued(&env, event_id, 1);
        Ok(ticket_id)
    }

    /// Sets the organizer's primary sale price for one of an event's tiers
    /// (issue #127). `purchase_primary` charges exactly this price, so the
    /// buyer can no longer choose what to pay — underpaying (including
    /// paying zero) is impossible. A price of 0 marks the tier as free.
    ///
    /// The organizer can change a tier price at any time; tickets already
    /// sold keep the price they were bought at in `original_price`, which is
    /// what resale caps and refunds are computed from.
    pub fn set_tier_price(
        env: Env,
        organizer: Address,
        event_id: u64,
        tier: String,
        price: i128,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        if price < 0 {
            return Err(Error::InvalidPrice);
        }
        let event = Self::get_event_inner(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        let key = DataKey::TierPrice(event_id, tier);
        env.storage().persistent().set(&key, &price);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Reads back the primary sale price configured for a tier (issue #127).
    /// Returns `TierPriceNotSet` when the organizer has not priced the tier.
    pub fn get_tier_price(env: Env, event_id: u64, tier: String) -> Result<i128, Error> {
        Self::tier_price(&env, event_id, &tier)
    }

    /// Internal lookup of an event's tier price, also extending the entry's
    /// TTL so a live sale never expires mid-transaction.
    fn tier_price(env: &Env, event_id: u64, tier: &String) -> Result<i128, Error> {
        let key = DataKey::TierPrice(event_id, tier.clone());
        let price: i128 = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::TierPriceNotSet)?;
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(price)
    }

    /// Buys a primary ticket at the event's tier price.
    ///
    /// The price is the organizer's configured `set_tier_price` value for the
    /// tier — it is no longer supplied by the caller, so a buyer cannot
    /// underpay a paid event (issue #127). Fails with `TierPriceNotSet` when
    /// the organizer has not priced that tier.
    pub fn purchase_primary(
        env: Env,
        buyer: Address,
        event_id: u64,
        tier: String,
        seat: String,
    ) -> Result<u64, Error> {
        Self::extend_instance_ttl(&env);
        buyer.require_auth();
        Self::require_initialized(&env)?;
        // Issue #126: bound tier/seat so they cannot inflate per-ticket
        // storage cost and rent. Validated before the throttle and the tier
        // price lookup so an oversized label is rejected on its own terms.
        Self::validate_ticket_labels(&tier, &seat)?;
        Self::enforce_purchase_throttle(&env, &buyer)?;
        let mut event = Self::get_event_inner(&env, event_id)?;
        let price = Self::tier_price(&env, event_id, &tier)?;
        let token_client = token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
        if price > 0 {
            if event.escrow_enabled {
                token_client.transfer(&buyer, env.current_contract_address(), &price);
                event.escrow_balance += price;
                env.storage()
                    .persistent()
                    .set(&DataKey::Event(event_id), &event);
            } else {
                token_client.transfer(&buyer, &event.organizer, &price);
            }
        }
        let ticket_id = Self::mint(&env, event_id, buyer.clone(), tier, seat, price);
        Self::increment_tickets_issued(&env, event_id, 1);
        Self::record_purchase(&env, &buyer);
        // `event` is a local copy, so the escrowed credit has to be written
        // back explicitly. Without this the running total is discarded:
        // `escrow_balance` stays at 0 in storage, `release_escrow` reads 0,
        // and every escrowed sale is stranded in the contract forever.
        if event.escrow_balance > 0 {
            Self::save_event(&env, event_id, &event);
        }
        Ok(ticket_id)
    }

    /// Releases an event's escrowed primary sale proceeds to the organizer.
    /// Only callable by the organizer, and only once the current ledger
    /// sequence has reached `escrow_release_ledger` (i.e. the event has
    /// ended).
    pub fn release_escrow(env: Env, organizer: Address, event_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::get_event_inner(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        if !event.escrow_enabled {
            return Err(Error::EscrowNotEnabled);
        }
        if env.ledger().sequence() < event.escrow_release_ledger {
            return Err(Error::EventNotEnded);
        }
        let amount = event.escrow_balance;
        event.escrow_balance = 0;
        Self::save_event(&env, event_id, &event);
        if amount > 0 {
            let token_client =
                token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
            token_client.transfer(&env.current_contract_address(), &organizer, &amount);
        }
        Ok(())
    }

    /// Admin-configurable minimum number of ledgers a buyer must wait
    /// between primary purchases. Set to 0 to disable the throttle.
    pub fn set_purchase_throttle(
        env: Env,
        admin: Address,
        min_ledger_spacing: u32,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::MinPurchaseSpacing, &min_ledger_spacing);
        PurchaseThrottleUpdated {
            admin,
            min_ledger_spacing,
        }
        .publish(&env);
        Ok(())
    }

    /// Direct, non-marketplace transfer (gift, family member, etc).
    pub fn transfer_ticket(
        env: Env,
        from: Address,
        ticket_id: u64,
        to: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        from.require_auth();
        // Issue #128: reject a self-transfer. It would otherwise consume a
        // slot against the per-ticket transfer limit and clear the resale
        // price and any gift claim without moving ownership.
        if from == to {
            return Err(Error::SelfTransfer);
        }
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != from {
            return Err(Error::NotOwner);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_transfer_allowed(&ticket, &event)?;
        if Self::transfer_frozen(&env, &event) {
            return Err(Error::TransfersFrozen);
        }
        ticket.owner = to;
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Direct, non-marketplace batch transfer of tickets to a single recipient.
    /// Bounded by `MAX_BATCH_SIZE`.
    pub fn transfer_batch(
        env: Env,
        from: Address,
        ticket_ids: Vec<u64>,
        to: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        from.require_auth();
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        for ticket_id in ticket_ids.iter() {
            let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
            if ticket.owner != from {
                return Err(Error::NotOwner);
            }
            match ticket.status {
                TicketStatus::Used => return Err(Error::AlreadyUsed),
                TicketStatus::Revoked => return Err(Error::Revoked),
                _ => {}
            }
            let event = Self::get_event_inner(&env, ticket.event_id)?;
            if Self::transfer_frozen(&env, &event) {
                return Err(Error::TransfersFrozen);
            }
            Self::ensure_transfer_allowed(&ticket, &event)?;
            ticket.owner = to.clone();
            ticket.transfers += 1;
            ticket.status = TicketStatus::Valid;
            ticket.resale_price = 0;
            Self::remove_gift_claim(&env, ticket_id);
            Self::save_ticket(&env, ticket_id, &ticket);
        }
        Ok(())
    }

    /// Creates a claim link without requiring the recipient address up front.
    /// The owner shares the preimage off-chain; only its SHA-256 digest is stored.
    pub fn create_gift_claim(
        env: Env,
        owner: Address,
        ticket_id: u64,
        secret_hash: BytesN<32>,
        expires_at: u64,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        if expires_at <= env.ledger().timestamp() {
            return Err(Error::InvalidExpiry);
        }

        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }
        if ticket.status == TicketStatus::Resale {
            ticket.status = TicketStatus::Valid;
            ticket.resale_price = 0;
            Self::save_ticket(&env, ticket_id, &ticket);
        }

        let claim = GiftClaim {
            from: owner,
            secret_hash,
            expires_at,
        };
        let key = DataKey::GiftClaim(ticket_id);
        env.storage().persistent().set(&key, &claim);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Claims a gifted ticket by presenting the secret preimage before expiry.
    pub fn claim_gift(
        env: Env,
        recipient: Address,
        ticket_id: u64,
        secret: Bytes,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        recipient.require_auth();
        let key = DataKey::GiftClaim(ticket_id);
        let claim: GiftClaim = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::GiftClaimNotFound)?;

        if env.ledger().timestamp() >= claim.expires_at {
            return Err(Error::GiftClaimExpired);
        }
        if env.crypto().sha256(&secret).to_bytes() != claim.secret_hash {
            return Err(Error::InvalidSecret);
        }

        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != claim.from {
            return Err(Error::NotOwner);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }

        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if Self::transfer_frozen(&env, &event) {
            return Err(Error::TransfersFrozen);
        }
        Self::ensure_transfer_allowed(&ticket, &event)?;

        ticket.owner = recipient;
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        env.storage().persistent().remove(&key);
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Read-only compatibility alias for [`Self::get_ticket`].
    ///
    /// `get_ticket` is the canonical Soroban-style name for this lookup.
    /// Keep this entry point for clients deployed against earlier interfaces;
    /// new integrations should call `get_ticket`.
    #[deprecated(note = "use get_ticket; verify_ticket is retained for ABI compatibility")]
    pub fn verify_ticket(env: Env, ticket_id: u64) -> Result<Ticket, Error> {
        Self::require_initialized(&env)?;
        Self::get_ticket_inner(&env, ticket_id)
    }

    /// Returns whether a ticket exists, belongs to `owner`, and is valid for
    /// entry. Missing tickets return `false` so scanners can use this as a
    /// single boolean check without handling a contract error.
    pub fn is_valid(env: Env, ticket_id: u64, owner: Address) -> bool {
        match Self::get_ticket_inner(&env, ticket_id) {
            Ok(ticket) => ticket.owner == owner && ticket.status == TicketStatus::Valid,
            Err(_) => false,
        }
    }

    /// Read-only on-chain batch verification of tickets.
    /// Allows scanners to inspect multiple tickets in one call.
    /// Bounded by `MAX_BATCH_SIZE`.
    pub fn verify_tickets(env: Env, ticket_ids: Vec<u64>) -> Result<Vec<Ticket>, Error> {
        Self::require_initialized(&env)?;
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        let mut tickets = Vec::new(&env);
        for ticket_id in ticket_ids.iter() {
            tickets.push_back(Self::get_ticket_inner(&env, ticket_id)?);
        }
        Ok(tickets)
    }

    /// Marks a ticket as used at the point of entry. Only the event's
    /// organizer (or their delegated gate device, via a shared Soroban
    /// signer) may check a ticket in, and only once.
    ///
    /// Tickets listed for resale (`TicketStatus::Resale`) cannot be checked in
    /// until the owner cancels the listing (`cancel_resale`) or completes the sale
    /// (issue #132).
    ///
    /// # Errors
    ///
    /// Returns `Error::ResaleListingActive` if the ticket is currently listed for resale.
    pub fn check_in(env: Env, organizer: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            TicketStatus::Resale => return Err(Error::ResaleListingActive),
            _ => {}
        }
        ticket.status = TicketStatus::Used;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        TicketCheckedIn {
            ticket_id,
            organizer,
        }
        .publish(&env);
        Ok(())
    }

    /// Marks a batch of tickets as used at the point of entry for group admission.
    /// Only the event's organizer may check tickets in, bounded by `MAX_BATCH_SIZE`.
    ///
    /// Tickets listed for resale (`TicketStatus::Resale`) are rejected (issue #132).
    ///
    /// # Errors
    ///
    /// Returns `Error::ResaleListingActive` if any ticket in the batch is currently listed for resale.
    pub fn check_in_batch(env: Env, organizer: Address, ticket_ids: Vec<u64>) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        for ticket_id in ticket_ids.iter() {
            let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
            let event = Self::get_event_inner(&env, ticket.event_id)?;
            if event.organizer != organizer {
                return Err(Error::NotOrganizer);
            }
            match ticket.status {
                TicketStatus::Used => return Err(Error::AlreadyUsed),
                TicketStatus::Revoked => return Err(Error::Revoked),
                TicketStatus::Resale => return Err(Error::ResaleListingActive),
                _ => {}
            }
            ticket.status = TicketStatus::Used;
            ticket.resale_price = 0;
            Self::remove_gift_claim(&env, ticket_id);
            Self::save_ticket(&env, ticket_id, &ticket);
            TicketCheckedIn {
                ticket_id,
                organizer: organizer.clone(),
            }
            .publish(&env);
        }
        Ok(())
    }

    /// Fraud prevention: organizer voids a ticket (chargeback, counterfeit
    /// report, policy violation). Revoked tickets can never be transferred,
    /// resold, or checked in again.
    ///
    /// # Errors
    ///
    /// Returns `Error::Revoked` if the ticket has already been revoked.
    pub fn revoke_ticket(env: Env, organizer: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        if ticket.status == TicketStatus::Revoked {
            return Err(Error::Revoked);
        }
        ticket.status = TicketStatus::Revoked;
        // A revoked ticket must not keep a live resale asking price (issue
        // #129): `resale_price` is the marker a listing is read from, so
        // leaving it non-zero kept listing data attached to a dead ticket.
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Updates a ticket's seat assignment. Only the organizer of the ticket's
    /// event may reassign it, and the ticket remains in its current state.
    pub fn set_seat(
        env: Env,
        organizer: Address,
        ticket_id: u64,
        seat: String,
    ) -> Result<(), Error> {
        organizer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        if ticket.status == TicketStatus::Revoked {
            return Err(Error::Revoked);
        }
        ticket.seat = seat;
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Revokes a ticket with an OPTIONAL refund to the current owner
    /// (issue #236). When `refund` is true, the organizer pays the ticket's
    /// `original_price` back to the owner in the event's accepted payment
    /// token before the ticket is voided — the revocation remains
    /// permanent afterwards. Without a refund, behavior matches
    /// `revoke_ticket`. Revoking a used ticket is rejected either way.
    ///
    /// # Errors
    ///
    /// Returns `Error::Revoked` if the ticket has already been revoked.
    pub fn revoke_with_refund(
        env: Env,
        organizer: Address,
        ticket_id: u64,
        refund: bool,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        if ticket.status == TicketStatus::Used {
            return Err(Error::AlreadyUsed);
        }
        if ticket.status == TicketStatus::Revoked {
            return Err(Error::Revoked);
        }
        if refund && ticket.original_price > 0 {
            let token_client =
                token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
            token_client.transfer(&organizer, &ticket.owner, &ticket.original_price);
        }
        ticket.status = TicketStatus::Revoked;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Mass revocation of tickets by the event organizer (chargeback, policy violation).
    /// Bounded by `MAX_BATCH_SIZE`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Revoked` if any ticket in the batch has already been revoked.
    pub fn revoke_batch(env: Env, organizer: Address, ticket_ids: Vec<u64>) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        for ticket_id in ticket_ids.iter() {
            let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
            let event = Self::get_event_inner(&env, ticket.event_id)?;
            if event.organizer != organizer {
                return Err(Error::NotOrganizer);
            }
            if ticket.status == TicketStatus::Revoked {
                return Err(Error::Revoked);
            }
            ticket.status = TicketStatus::Revoked;
            Self::remove_gift_claim(&env, ticket_id);
            Self::save_ticket(&env, ticket_id, &ticket);
        }
        Ok(())
    }

    /// Lists an owned, valid ticket on the resale marketplace. The price is
    /// capped at `original_price * max_resale_multiplier_bps / 10_000` using
    /// integer division (truncating toward zero / rounding down). When the
    /// intermediate product is not evenly divisible by 10_000, fractional
    /// amounts round down so the effective cap never exceeds the nominal cap.
    pub fn list_for_resale(
        env: Env,
        owner: Address,
        ticket_id: u64,
        price: i128,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        let cap = ticket.original_price * event.max_resale_multiplier_bps as i128
            / BPS_DENOMINATOR as i128;
        if price > cap {
            return Err(Error::ResalePriceExceedsCap);
        }
        if let Some(floor_bps) = event.min_resale_multiplier_bps {
            let floor = ticket.original_price * floor_bps as i128 / 10_000;
            if price < floor {
                return Err(Error::ResalePriceBelowFloor);
            }
        }
        ticket.status = TicketStatus::Resale;
        ticket.resale_price = price;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Cancels a ticket's resale listing, returning its status to valid.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotOwner` if the caller is not the ticket's owner.
    /// Returns `Error::NotForResale` if the ticket is not currently listed for resale.
    pub fn cancel_resale(env: Env, owner: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Buys a resale-listed ticket. Payment is settled on-chain: the
    /// organizer's royalty cut and the seller's remainder both move out of
    /// the buyer's token balance, and ownership transfers to the buyer.
    ///
    /// # Ordering (issue #133)
    ///
    /// This entry point pays out to two addresses the caller does not
    /// control, by invoking the event's payment token twice. An event's
    /// payment token is only checked for exposing `decimals`, so it is not
    /// necessarily a Stellar asset contract: it is untrusted code running in
    /// the middle of this function. The body is therefore ordered checks,
    /// then effects, then interactions -- the ticket is fully updated and
    /// persisted *before* the first `transfer`, so no external call ever
    /// observes a half-settled purchase.
    ///
    /// This is defence in depth, not a fix for a live exploit, and it is
    /// worth being accurate about which. The issue that prompted it proposed
    /// a malicious payment token that re-enters `buy_resale` from inside
    /// `transfer` and buys the same listing twice. That is not reachable on
    /// Soroban: the host refuses to re-enter a contract that is already
    /// executing, returning `Error(Context, InvalidAction)`, and it does so
    /// for a plain `get_ticket` read as much as for a write. An attempted
    /// test of the double purchase passes on the old ordering for exactly
    /// that reason, which is why the ordering is justified by the invariant
    /// below rather than by a regression test.
    ///
    /// The invariant is worth keeping regardless of the host's behaviour,
    /// because the guarantee the host provides is narrow -- it is about
    /// re-entering *this* contract, and says nothing about the state this
    /// contract has committed or about what an observer of the ledger sees
    /// between the two transfers. Under the previous ordering, the stored
    /// ticket still read `Resale` with the seller as owner and a live
    /// `resale_price` while the organizer had already been paid, and
    /// `remove_gift_claim` was a state change made after the external calls.
    /// The host would still stop a re-entrant call, but a contract that
    /// depends on that is depending on a platform behaviour rather than on
    /// its own ordering.
    ///
    /// The royalty and seller amounts are computed and captured before the
    /// ticket is overwritten, because `seller` has to be the pre-sale owner
    /// and `ticket.owner` is reassigned to the buyer above.
    pub fn buy_resale(env: Env, buyer: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        buyer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        // Issue #128: a seller must not be able to buy their own listing,
        // which would move the ticket to itself, consume a transfer slot and
        // pay a royalty to the organizer for nothing.
        if ticket.owner == buyer {
            return Err(Error::SelfPurchase);
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        Self::ensure_transfer_allowed(&ticket, &event)?;
        let token_address = Self::payment_token_for_event(&env, &event)?;

        // --- effects: settle state before any untrusted external call. ---
        let royalty = ticket.resale_price * event.royalty_bps as i128 / BPS_DENOMINATOR as i128;
        let seller_amount = ticket.resale_price - royalty;
        let seller = ticket.owner.clone();
        ticket.owner = buyer.clone();
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);

        // --- interactions: the only untrusted calls, after state is final.
        let token_client = token::Client::new(&env, &token_address);
        if royalty > 0 {
            token_client.transfer(&buyer, &event.organizer, &royalty);
        }
        if seller_amount > 0 {
            token_client.transfer(&buyer, &seller, &seller_amount);
        }
        Ok(())
    }

    /// Fetches an event by its id.
    ///
    /// # Errors
    ///
    /// Returns `Error::EventNotFound` when no event with `event_id` exists.
    pub fn get_event(env: Env, event_id: u64) -> Result<Event, Error> {
        Self::require_initialized(&env)?;
        Self::get_event_inner(&env, event_id)
    }

    /// Fetches a ticket by its id.
    ///
    /// # Errors
    ///
    /// Returns `Error::TicketNotFound` when no ticket with `ticket_id` exists.
    pub fn get_ticket(env: Env, ticket_id: u64) -> Result<Ticket, Error> {
        Self::require_initialized(&env)?;
        Self::get_ticket_inner(&env, ticket_id)
    }

    /// Permissionless public storage-rent renewal for a ticket record.
    ///
    /// Anyone can pay transaction fees to keep a ticket alive, but the call
    /// does not mutate ownership, lifecycle, pricing, or event state.
    pub fn extend_ticket_ttl(env: Env, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_initialized(&env)?;
        Self::bump_ticket_ttl(&env, ticket_id)
    }

    /// Returns the number of events registered by `organizer`.
    pub fn get_organizer_events(env: Env, organizer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::OrganizerEvents(organizer))
            .unwrap_or(0)
    }

    fn get_event_inner(env: &Env, event_id: u64) -> Result<Event, Error> {
        let mut event: Event = env
            .storage()
            .persistent()
            .get(&DataKey::Event(event_id))
            .ok_or(Error::EventNotFound)?;
        // The fallback keeps events created before the counter migration
        // readable; all newly created events have their own counter key.
        event.tickets_issued = env
            .storage()
            .persistent()
            .get(&DataKey::TicketsIssued(event_id))
            .unwrap_or(event.tickets_issued);
        Ok(event)
    }

    fn get_ticket_inner(env: &Env, ticket_id: u64) -> Result<Ticket, Error> {
        let key = DataKey::Ticket(ticket_id);
        let stored: StoredTicket = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::TicketNotFound)?;
        Self::unpack_ticket(stored)
    }

    fn bump_ticket_ttl(env: &Env, ticket_id: u64) -> Result<(), Error> {
        let key = DataKey::Ticket(ticket_id);
        if !env.storage().persistent().has(&key) {
            return Err(Error::TicketNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    fn increment_tickets_issued(env: &Env, event_id: u64, amount: u64) {
        let key = DataKey::TicketsIssued(event_id);
        let current = env.storage().persistent().get(&key).unwrap_or(0u64);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_add(amount));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn increment_organizer_events(env: &Env, organizer: &Address) {
        let key = DataKey::OrganizerEvents(organizer.clone());
        let count = env.storage().persistent().get(&key).unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&key, &count.saturating_add(1));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Writes an event and refreshes its TTL in the same step.
    ///
    /// Every write to an `Event` must go through here. `storage().set` on its
    /// own leaves the entry's `live_until_ledger` exactly where it was, so a
    /// path that updates an event without extending it leaves the event --
    /// and every ticket that can only be resolved through it -- sitting on
    /// the expiry it happened to be created with (issue #139).
    fn save_event(env: &Env, event_id: u64, event: &Event) {
        let key = DataKey::Event(event_id);
        env.storage().persistent().set(&key, event);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn save_ticket(env: &Env, ticket_id: u64, ticket: &Ticket) {
        let key = DataKey::Ticket(ticket_id);
        env.storage()
            .persistent()
            .set(&key, &Self::pack_ticket(ticket));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn remove_gift_claim(env: &Env, ticket_id: u64) {
        env.storage()
            .persistent()
            .remove(&DataKey::GiftClaim(ticket_id));
    }

    fn transfer_frozen(env: &Env, event: &Event) -> bool {
        env.ledger().timestamp()
            >= event
                .starts_at
                .saturating_sub(event.transfer_freeze_seconds)
    }

    fn resale_closed(env: &Env, event: &Event) -> bool {
        env.ledger().timestamp() >= event.starts_at.saturating_sub(event.resale_cutoff_seconds)
    }

    fn ensure_transfer_allowed(ticket: &Ticket, event: &Event) -> Result<(), Error> {
        if let Some(max_transfers) = event.max_transfers_per_ticket {
            if ticket.transfers >= max_transfers {
                return Err(Error::TransferLimitExceeded);
            }
        }
        Ok(())
    }

    fn enforce_purchase_throttle(env: &Env, buyer: &Address) -> Result<(), Error> {
        let spacing: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MinPurchaseSpacing)
            .unwrap_or(0);
        if spacing == 0 {
            return Ok(());
        }
        let key = DataKey::LastPurchaseLedger(buyer.clone());
        if let Some(last_ledger) = env.storage().persistent().get::<_, u32>(&key) {
            let current = env.ledger().sequence();
            if current.saturating_sub(last_ledger) < spacing {
                return Err(Error::PurchaseTooSoon);
            }
        }
        Ok(())
    }

    fn record_purchase(env: &Env, buyer: &Address) {
        let key = DataKey::LastPurchaseLedger(buyer.clone());
        let current = env.ledger().sequence();
        env.storage().persistent().set(&key, &current);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Rejects event creation by an address that is not on the organizer
    /// allowlist (issue #135). Called after `require_auth()` so the
    /// auth-before-business-validation ordering is preserved.
    fn require_approved_organizer(env: &Env, organizer: &Address) -> Result<(), Error> {
        let key = DataKey::ApprovedOrganizer(organizer.clone());
        if !env.storage().persistent().has(&key) {
            return Err(Error::OrganizerNotApproved);
        }
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if stored_admin != *admin {
            return Err(Error::NotAdmin);
        }
        admin.require_auth();
        Ok(())
    }

    /// Probes `token` with a `decimals()` call so an address that is not a
    /// token contract is rejected up front. Returns the token's decimals so
    /// callers can cache them (issue #233).
    fn ensure_token_contract(env: &Env, token: &Address) -> Result<u32, Error> {
        match token::Client::new(env, token).try_decimals() {
            Ok(Ok(decimals)) => Ok(decimals),
            _ => Err(Error::InvalidPaymentToken),
        }
    }

    /// Decimals of the active payment token, cached at initialization and
    /// refreshed whenever the payment token changes. Frontends use this to
    /// convert token amounts between raw units and display units.
    pub fn token_decimals(env: Env) -> Result<u32, Error> {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TokenDecimals)
            .ok_or(Error::NotInitialized)
    }

    fn payment_token(env: &Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::PaymentToken)
            .ok_or(Error::NotInitialized)
    }

    /// Resolves the payment token for an event: the per-event accepted token
    /// when set, otherwise the contract-wide payment token (issue #235).
    fn payment_token_for_event(env: &Env, event: &Event) -> Result<Address, Error> {
        match &event.payment_token {
            Some(token) => Ok(token.clone()),
            None => Self::payment_token(env),
        }
    }

    fn mint(env: &Env, event_id: u64, to: Address, tier: String, seat: String, price: i128) -> u64 {
        let ticket_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextTicketId)
            .unwrap_or(0);
        let ticket = Ticket {
            event_id,
            owner: to,
            tier,
            seat,
            status: TicketStatus::Valid,
            original_price: price,
            resale_price: 0,
            transfers: 0,
        };
        Self::save_ticket(env, ticket_id, &ticket);
        env.storage()
            .instance()
            .set(&DataKey::NextTicketId, &(ticket_id + 1));
        TicketIssued {
            ticket_id,
            event_id,
        }
        .publish(env);
        ticket_id
    }

    fn pack_ticket(ticket: &Ticket) -> StoredTicket {
        StoredTicket {
            event_id: ticket.event_id,
            owner: ticket.owner.clone(),
            tier: ticket.tier.clone(),
            seat: ticket.seat.clone(),
            lifecycle: ((ticket.transfers as u64) << 8) | Self::status_code(&ticket.status),
            original_price: ticket.original_price,
            resale_price: ticket.resale_price,
        }
    }

    fn unpack_ticket(stored: StoredTicket) -> Result<Ticket, Error> {
        let status_code = stored.lifecycle & 0xff;
        let transfers = (stored.lifecycle >> 8) as u32;
        Ok(Ticket {
            event_id: stored.event_id,
            owner: stored.owner,
            tier: stored.tier,
            seat: stored.seat,
            status: Self::status_from_code(status_code)?,
            original_price: stored.original_price,
            resale_price: stored.resale_price,
            transfers,
        })
    }

    fn status_code(status: &TicketStatus) -> u64 {
        match status {
            TicketStatus::Valid => 0,
            TicketStatus::Used => 1,
            TicketStatus::Revoked => 2,
            TicketStatus::Resale => 3,
        }
    }

    fn status_from_code(code: u64) -> Result<TicketStatus, Error> {
        match code {
            0 => Ok(TicketStatus::Valid),
            1 => Ok(TicketStatus::Used),
            2 => Ok(TicketStatus::Revoked),
            3 => Ok(TicketStatus::Resale),
            _ => Err(Error::InvalidTicketLifecycle),
        }
    }
}

#[cfg(test)]
mod test;
