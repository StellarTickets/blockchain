//! StellarTickets ticketing smart contract for Soroban on Stellar.
#![no_std]
#![allow(missing_docs)]
#![allow(clippy::pedantic)]

mod constants;
mod error;
mod events;
mod types;

pub use constants::{
    BPS_DENOMINATOR, CATEGORY_CONCERT, CATEGORY_CONFERENCE, CATEGORY_FESTIVAL, CATEGORY_FLIGHT,
    CATEGORY_OTHER, CATEGORY_SPORTS, MAX_BATCH_SIZE, MAX_CATEGORY_LEN, MAX_EVENT_METADATA_ENTRIES,
    MAX_EVENT_METADATA_KEY_LEN, MAX_EVENT_METADATA_VALUE_LEN, MAX_METADATA_URI_LEN, MAX_NAME_LEN,
    MAX_OWNER_HISTORY, MAX_PAGE_SIZE, MAX_PERK_LEN, MAX_TICKET_LABEL_LEN, MAX_TIER_PERKS,
    MAX_TIER_ZONES, PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS, USED_TICKET_RETENTION_SECONDS,
};
use constants::{LEDGER_BUMP, LEDGER_THRESHOLD};
pub use error::Error;
pub use events::{
    AuctionSettled, AuctionStarted, BidPlaced, ContractInitialized, EventCancelled, EventCreated,
    ListingCancelled, OfferAccepted, OfferCancelled, OfferMade, OrganizerApproved,
    OrganizerRevoked, PaymentTokenChanged, PaymentTokenProposed, PlatformFeeUpdated, PrimarySale,
    PurchaseThrottleUpdated, TicketCheckedIn, TicketIssued, TicketListed, TicketResold,
    TicketTransferred,
};
pub use types::{
    Auction, Category, DataKey, Event, EventParams, GiftClaim, Offer, PendingPaymentToken,
    StoredTicket, Ticket, TicketRecipient, TicketStatus,
};

use soroban_sdk::{contract, contractimpl, token, Address, Bytes, BytesN, Env, Map, String, Vec};

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
        Self::require_not_paused(&env)?;
        Self::require_admin(&env, &admin)?;
        // The probe validates that the proposed token is a real token
        // contract. Its decimals are captured when the change is APPLIED so
        // clients always see decimals matching the active payment token
        // (issue #233).
        Self::ensure_token_contract(&env, &new_token)?;
        // Reject a no-op proposal: proposing the token that is already active
        // would start the timelock and emit an event for a change that does
        // nothing (issue #365).
        if let Ok(current) = Self::payment_token(&env) {
            if new_token == current {
                return Err(Error::SamePaymentToken);
            }
        }
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
        Self::require_not_paused(&env)?;
        Self::require_admin(&env, &admin)?;
        let pending: PendingPaymentToken = env
            .storage()
            .instance()
            .get(&DataKey::PendingPaymentToken)
            .ok_or(Error::NoPendingPaymentToken)?;
        if env.ledger().sequence() < pending.apply_after_ledger {
            return Err(Error::TimelockNotElapsed);
        }
        let outstanding_escrow = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::OutstandingEscrow)
            .unwrap_or(0);
        if outstanding_escrow > 0 {
            return Err(Error::EscrowNotEnabled);
        }
        // Bids and offers are held in the active token, so they must settle or
        // be refunded before the token can change (issues #105, #106).
        if Self::resale_funds_outstanding(&env) > 0 {
            return Err(Error::ResaleFundsOutstanding);
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

    /// Approves an organizer so they may create events. Idempotent: repeated
    /// approvals leave the allowlist unchanged and just re-emit the event.
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

    /// Registers a new event under an organizer. `event_id` is
    /// chosen by the caller's backend (e.g. a ULID cast to u64) so it can be
    /// correlated with the off-chain event record. Only organizers on the
    /// admin-managed allowlist may call this, so an arbitrary address cannot
    /// squat on an id the backend is about to use (issue #135).
    pub fn create_event(
        env: Env,
        organizer: Address,
        event_id: u64,
        params: EventParams,
    ) -> Result<(), Error> {
        Self::create_event_with_options(env, organizer, event_id, params, None, None)
    }

    /// Registers an event with optional resale floor and transfer limit.
    /// Kept separate from `create_event` to preserve a concise default
    /// entrypoint when optional controls are not needed.
    pub fn create_event_with_options(
        env: Env,
        organizer: Address,
        event_id: u64,
        params: EventParams,
        min_resale_multiplier_bps: Option<u32>,
        max_transfers_per_ticket: Option<u32>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_initialized(&env)?;
        Self::require_not_paused(&env)?;
        Self::require_approved_organizer(&env, &organizer)?;
        if params.royalty_bps > 10_000 {
            return Err(Error::InvalidRoyalty);
        }
        // Issue #124: a multiplier below face value (10_000 bps) makes every
        // resale impossible — `list_for_resale` computes the cap as
        // `original_price * max_resale_multiplier_bps / 10_000`, which lands
        // below the face value and rejects any listing at or above it. Require
        // the multiplier to be at least face value.
        if params.max_resale_multiplier_bps < BPS_DENOMINATOR {
            return Err(Error::InvalidMultiplier);
        }
        // Issue #125: reject empty or oversized name/category.
        Self::validate_event_labels(&params.name, &params.category)?;
        if params.starts_at <= env.ledger().timestamp() {
            return Err(Error::InvalidEventTime);
        }
        let key = DataKey::Event(event_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::EventAlreadyExists);
        }
        if let (Some(presale_starts), Some(presale_ends)) =
            (params.presale_starts_at, params.presale_ends_at)
        {
            if presale_starts >= presale_ends || presale_ends > params.starts_at {
                return Err(Error::InvalidEventTime);
            }
        }
        let event = Event {
            organizer,
            name: params.name,
            category: params.category,
            max_resale_multiplier_bps: params.max_resale_multiplier_bps,
            min_resale_multiplier_bps,
            max_transfers_per_ticket,
            royalty_bps: params.royalty_bps,
            tickets_issued: 0,
            starts_at: params.starts_at,
            transfer_freeze_seconds: params.transfer_freeze_seconds,
            resale_cutoff_seconds: params.resale_cutoff_seconds,
            escrow_enabled: false,
            escrow_release_ledger: 0,
            escrow_balance: 0,
            payment_token: None,
            transferable: true,
            max_per_wallet: params.max_per_wallet,
            refund_window_seconds: params.refund_window_seconds,
            presale_starts_at: params.presale_starts_at,
            presale_ends_at: params.presale_ends_at,
            max_tickets: None,
            check_in_opens_seconds: None,
            ends_at: None,
            ticket_expires_at: None,
            max_reentries: 0,
            cancelled: false,
            royalty_splits: None,
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
        Self::index_organizer_event(&env, &event.organizer, event_id);
        EventCreated {
            organizer: event.organizer,
            event_id,
        }
        .publish(&env);
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
        Self::require_not_paused(&env)?;
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
        let event = Self::require_organizer(&env, event_id, &organizer)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_capacity(&event, winner_count as u64)?;
        Self::reserve_tier_capacity(&env, event_id, &tier, winner_count as u64)?;

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
        Self::require_not_paused(&env)?;
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if event.tickets_issued > 0 {
            return Err(Error::EventAlreadyStarted);
        }
        if escrow_release_ledger <= env.ledger().sequence() {
            return Err(Error::EventNotEnded);
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
        Self::require_not_paused(&env)?;
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
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
        let event = Self::get_event_inner(&env, event_id)?;
        Self::payment_token_for_event(&env, &event)
    }

    /// Makes an event's tickets soulbound or transferable (issue #108). When
    /// `transferable` is false, direct transfers, gift claims, resale listings
    /// and resale purchases all reject with `NotTransferable`. Only callable
    /// by the organizer before any ticket is issued, so a buyer never acquires
    /// a ticket under different terms from the ones they bought it on.
    pub fn set_event_transferable(
        env: Env,
        organizer: Address,
        event_id: u64,
        transferable: bool,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if event.tickets_issued > 0 {
            return Err(Error::TicketsAlreadyIssued);
        }
        event.transferable = transferable;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Cancels an event (issue #91). A cancelled event rejects new issuance,
    /// primary sales and resale listings. Tickets already issued are left
    /// untouched, so existing holders keep their tickets.
    ///
    /// # Errors
    ///
    /// Returns `Error::EventCancelled` if the event is already cancelled.
    pub fn cancel_event(env: Env, organizer: Address, event_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if event.cancelled {
            return Err(Error::EventCancelled);
        }
        event.cancelled = true;
        Self::save_event(&env, event_id, &event);
        EventCancelled {
            event_id,
            organizer,
        }
        .publish(&env);
        Ok(())
    }

    /// Splits the event's resale royalty between several recipients (issue
    /// #90). Each entry is `(recipient, bps)` and the entries must sum to the
    /// event's `royalty_bps`. `None` restores the default of paying the whole
    /// royalty to the organizer. Applies to resales settled after the change.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidRoyalty` when the splits are empty or do not sum
    /// to `royalty_bps`.
    pub fn set_royalty_splits(
        env: Env,
        organizer: Address,
        event_id: u64,
        splits: Option<Vec<(Address, u32)>>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if let Some(entries) = &splits {
            if entries.is_empty() {
                return Err(Error::InvalidRoyalty);
            }
            let mut total: u64 = 0;
            for (_, bps) in entries.iter() {
                total += bps as u64;
            }
            if total != event.royalty_bps as u64 {
                return Err(Error::InvalidRoyalty);
            }
        }
        event.royalty_splits = splits;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Caps how many tickets an event may ever issue (issue #92). `None`
    /// removes the cap. The cap cannot be set below `tickets_issued`, since
    /// those tickets already exist.
    pub fn set_event_capacity(
        env: Env,
        organizer: Address,
        event_id: u64,
        max_tickets: Option<u64>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if let Some(max) = max_tickets {
            if max == 0 || max < event.tickets_issued {
                return Err(Error::InvalidCapacity);
            }
        }
        event.max_tickets = max_tickets;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Configures the check-in window (issue #95). Check-in opens
    /// `check_in_opens_seconds` before `starts_at` (`None` opens it from the
    /// start of the event) and closes at `ends_at` (`None` never closes).
    /// `ends_at` must fall after `starts_at`.
    pub fn set_event_check_in_window(
        env: Env,
        organizer: Address,
        event_id: u64,
        check_in_opens_seconds: Option<u64>,
        ends_at: Option<u64>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if let Some(end) = ends_at {
            if end <= event.starts_at {
                return Err(Error::InvalidCheckInWindow);
            }
        }
        event.check_in_opens_seconds = check_in_opens_seconds;
        event.ends_at = ends_at;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Sets when the event's tickets expire (issue #76). Expired tickets can
    /// no longer be checked in or transferred. `None` removes the expiry. The
    /// expiry must be in the future when set.
    pub fn set_event_ticket_expiry(
        env: Env,
        organizer: Address,
        event_id: u64,
        expires_at: Option<u64>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        if let Some(expires) = expires_at {
            if expires <= env.ledger().timestamp() {
                return Err(Error::InvalidTicketExpiry);
            }
        }
        event.ticket_expires_at = expires_at;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Sets how many times a checked-in ticket may re-enter the venue (issue
    /// #78). `0` disables re-entry for the event.
    pub fn set_event_max_reentries(
        env: Env,
        organizer: Address,
        event_id: u64,
        max_reentries: u32,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
        event.max_reentries = max_reentries;
        Self::save_event(&env, event_id, &event);
        Ok(())
    }

    /// Passes a checked-in ticket back out so it can be checked in again
    /// (issue #78). Only the event's organizer may do this, and only when the
    /// event allows re-entry and the ticket has not used all of its
    /// re-entries. Returns the ticket's re-entry count after this pass-out.
    ///
    /// # Errors
    ///
    /// Returns `Error::TicketNotUsed` if the ticket is not checked in,
    /// `Error::ReentryNotAllowed` if the event disables re-entry, and
    /// `Error::ReentryLimitReached` once the ticket has used all of its
    /// re-entries.
    pub fn pass_out(env: Env, organizer: Address, ticket_id: u64) -> Result<u32, Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::require_organizer(&env, ticket.event_id, &organizer)?;
        if ticket.status != TicketStatus::Used {
            return Err(Error::TicketNotUsed);
        }
        if event.max_reentries == 0 {
            return Err(Error::ReentryNotAllowed);
        }
        let key = DataKey::ReentryCount(ticket_id);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        if count >= event.max_reentries {
            return Err(Error::ReentryLimitReached);
        }
        let count = count + 1;
        // Passing out hands back the use the entry consumed (issue #74), so the
        // ticket is valid again with the same uses left as before it entered.
        let used = Self::used_count_of(&env, ticket_id);
        Self::store_used_count(&env, ticket_id, used.saturating_sub(1));
        ticket.status = TicketStatus::Valid;
        Self::save_ticket(&env, ticket_id, &ticket);
        env.storage().persistent().set(&key, &count);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(count)
    }

    /// Number of times a ticket has been passed back out for re-entry
    /// (issue #78). Zero for tickets that never re-entered.
    pub fn reentry_count(env: Env, ticket_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ReentryCount(ticket_id))
            .unwrap_or(0)
    }

    /// Sets how many entries a ticket allows before it is fully used (issue
    /// #74). Tickets default to one use, which is the single-use behavior.
    /// Raising the limit on an exhausted ticket makes it valid again.
    ///
    /// # Errors
    ///
    /// Returns `Error::Revoked` if the ticket is revoked, and
    /// `Error::InvalidMaxUses` if `max_uses` is zero or below the uses already
    /// recorded.
    pub fn set_ticket_max_uses(
        env: Env,
        organizer: Address,
        ticket_id: u64,
        max_uses: u32,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        Self::require_organizer(&env, ticket.event_id, &organizer)?;
        if ticket.status == TicketStatus::Revoked {
            return Err(Error::Revoked);
        }
        let used = Self::used_count_of(&env, ticket_id);
        if max_uses == 0 || max_uses < used {
            return Err(Error::InvalidMaxUses);
        }
        let key = DataKey::MaxUses(ticket_id);
        env.storage().persistent().set(&key, &max_uses);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        if ticket.status != TicketStatus::Resale {
            ticket.status = if used >= max_uses {
                TicketStatus::Used
            } else {
                TicketStatus::Valid
            };
            Self::save_ticket(&env, ticket_id, &ticket);
        }
        Ok(())
    }

    /// Number of entries a ticket allows before it is fully used (issue #74).
    /// Defaults to one for tickets that never had a limit set.
    pub fn ticket_max_uses(env: Env, ticket_id: u64) -> u32 {
        Self::max_uses_of(&env, ticket_id)
    }

    /// Number of entries recorded against a ticket (issue #74). Zero for
    /// tickets that have not been checked in.
    pub fn ticket_used_count(env: Env, ticket_id: u64) -> u32 {
        Self::used_count_of(&env, ticket_id)
    }

    /// Organizer-authorized issuance for tickets already paid for off-chain
    /// (card payment, comp, or fiat-to-crypto settled by the platform).
    /// Increments the event's monotonic `tickets_issued` counter by 1.
    pub fn issue_ticket(
        env: Env,
        organizer: Address,
        event_id: u64,
        to: Address,
        tier: String,
        seat: String,
        price: i128,
    ) -> Result<u64, Error> {
        Self::issue_ticket_with_metadata(env, organizer, event_id, to, tier, seat, price, None)
    }

    /// Issues a ticket like `issue_ticket` and attaches an optional metadata
    /// URI at mint (issue #109). The URI is bounded by `MAX_METADATA_URI_LEN`
    /// and is read back through `token_uri`. Passing `None` mints a ticket
    /// without one, exactly as `issue_ticket` does.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_ticket_with_metadata(
        env: Env,
        organizer: Address,
        event_id: u64,
        to: Address,
        tier: String,
        seat: String,
        price: i128,
        metadata_uri: Option<String>,
    ) -> Result<u64, Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_initialized(&env)?;
        Self::require_not_paused(&env)?;
        if price < 0 {
            return Err(Error::InvalidPrice);
        }
        // Issue #126: bound tier/seat so they cannot inflate per-ticket
        // storage cost and rent.
        Self::validate_ticket_labels(&tier, &seat)?;
        if let Some(uri) = &metadata_uri {
            Self::validate_label(uri, MAX_METADATA_URI_LEN)?;
        }
        let event = Self::require_organizer(&env, event_id, &organizer)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_capacity(&event, 1)?;
        Self::reserve_tier_capacity(&env, event_id, &tier, 1)?;
        let ticket_id = Self::mint(&env, event_id, to, tier, seat, price);
        if let Some(uri) = &metadata_uri {
            Self::store_metadata_uri(&env, ticket_id, uri);
        }
        Self::increment_tickets_issued(&env, event_id, 1);
        Ok(ticket_id)
    }

    /// Mints many tickets for one event in a single call (issue #88). Every
    /// recipient goes through the same mint helper as `issue_ticket`, so each
    /// one emits its own `TicketIssued` event. Returns the new ticket ids in
    /// the same order as `recipients`.
    ///
    /// Bounded by `MAX_BATCH_SIZE` and atomic: if any recipient fails
    /// validation, no ticket from this call is minted.
    ///
    /// # Errors
    ///
    /// Returns `Error::EmptyBatch` for an empty list, `Error::BatchTooLarge`
    /// above `MAX_BATCH_SIZE`, and `Error::SoldOut` or `Error::TierSoldOut`
    /// when the batch would exceed the event or tier cap.
    pub fn issue_tickets(
        env: Env,
        organizer: Address,
        event_id: u64,
        recipients: Vec<TicketRecipient>,
    ) -> Result<Vec<u64>, Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_initialized(&env)?;
        if recipients.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if recipients.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        let event = Self::require_organizer(&env, event_id, &organizer)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_capacity(&event, recipients.len() as u64)?;
        let mut ticket_ids = Vec::new(&env);
        for recipient in recipients.iter() {
            if recipient.price < 0 {
                return Err(Error::InvalidPrice);
            }
            Self::validate_ticket_labels(&recipient.tier, &recipient.seat)?;
            Self::reserve_tier_capacity(&env, event_id, &recipient.tier, 1)?;
            let ticket_id = Self::mint(
                &env,
                event_id,
                recipient.to,
                recipient.tier,
                recipient.seat,
                recipient.price,
            );
            ticket_ids.push_back(ticket_id);
        }
        Self::increment_tickets_issued(&env, event_id, recipients.len() as u64);
        Ok(ticket_ids)
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
        Self::require_not_paused(&env)?;
        if price < 0 {
            return Err(Error::InvalidPrice);
        }
        Self::require_organizer(&env, event_id, &organizer)?;
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

    /// Caps how many tickets one of an event's tiers may ever issue (issue
    /// #93). `None` removes the cap. Every mint path counts toward the tier
    /// whether or not a cap is set, so the cap can be added at any time, but
    /// it cannot be set below the number of tickets already issued in the tier.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidCapacity` for a zero cap or one below the tier's
    /// issued count.
    pub fn set_tier_capacity(
        env: Env,
        organizer: Address,
        event_id: u64,
        tier: String,
        cap: Option<u64>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_organizer(&env, event_id, &organizer)?;
        Self::validate_label(&tier, MAX_TICKET_LABEL_LEN)?;
        let key = DataKey::TierCapacity(event_id, tier.clone());
        match cap {
            Some(max) => {
                let issued = Self::tier_issued(&env, event_id, &tier);
                if max == 0 || max < issued {
                    return Err(Error::InvalidCapacity);
                }
                env.storage().persistent().set(&key, &max);
                env.storage()
                    .persistent()
                    .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
            }
            None => env.storage().persistent().remove(&key),
        }
        Ok(())
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
    /// the organizer has not priced that tier. Increments the event's
    /// monotonic `tickets_issued` counter by 1.
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
        Self::require_not_paused(&env)?;
        Self::validate_ticket_labels(&tier, &seat)?;
        Self::enforce_purchase_throttle(&env, &buyer)?;
        let mut event = Self::get_event_inner(&env, event_id)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_sale_open(&env, event_id)?;
        Self::enforce_presale_rules(&env, event_id, &buyer, &event)?;
        Self::enforce_wallet_limit(&env, event_id, &buyer, &event)?;
        Self::ensure_capacity(&event, 1)?;
        let price = Self::tier_price(&env, event_id, &tier)?;
        Self::reserve_tier_capacity(&env, event_id, &tier, 1)?;
        let token_client = token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
        // The platform fee goes to the admin straight from the buyer. The
        // organizer's share is what escrow holds or the organizer receives, so
        // the escrow total still matches the proceeds owed to the organizer (issue #104).
        let fee = Self::platform_fee_on(&env, price)?;
        let proceeds = price - fee;
        if fee > 0 {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(Error::NotInitialized)?;
            token_client.transfer(&buyer, &admin, &fee);
        }
        if proceeds > 0 {
            if event.escrow_enabled {
                token_client.transfer(&buyer, env.current_contract_address(), &proceeds);
                event.escrow_balance += proceeds;
                let current_escrow = env
                    .storage()
                    .instance()
                    .get::<_, i128>(&DataKey::OutstandingEscrow)
                    .unwrap_or(0);
                env.storage()
                    .instance()
                    .set(&DataKey::OutstandingEscrow, &(current_escrow + proceeds));
                env.storage()
                    .persistent()
                    .set(&DataKey::Event(event_id), &event);
            } else {
                token_client.transfer(&buyer, &event.organizer, &proceeds);
            }
        }
        let ticket_id = Self::mint(&env, event_id, buyer.clone(), tier, seat, price);
        Self::increment_tickets_issued(&env, event_id, 1);
        Self::record_purchase(&env, &buyer);
        // The wallet counter is what `enforce_wallet_limit` compares against
        // `max_per_wallet`, so a sale has to be counted for the limit to bind.
        // A refund (`refund_primary`) and a refunded revocation
        // (`revoke_with_refund`) give the slot back.
        Self::increment_wallet_purchase_count(&env, event_id, &buyer);
        PrimarySale {
            ticket_id,
            buyer,
            price,
        }
        .publish(&env);
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
        Self::require_not_paused(&env)?;
        let mut event = Self::require_organizer(&env, event_id, &organizer)?;
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
            let current_escrow = env
                .storage()
                .instance()
                .get::<_, i128>(&DataKey::OutstandingEscrow)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::OutstandingEscrow, &(current_escrow - amount));
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

    /// Sets the platform fee taken from every primary sale, in basis points of
    /// the sale price (issue #104). `0` turns the fee off. The fee is paid to
    /// the admin when a sale settles, and the organizer receives the rest.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidPlatformFee` when `fee_bps` is above 10_000.
    pub fn set_platform_fee_bps(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        if fee_bps > BPS_DENOMINATOR {
            return Err(Error::InvalidPlatformFee);
        }
        env.storage()
            .instance()
            .set(&DataKey::PlatformFeeBps, &fee_bps);
        PlatformFeeUpdated { admin, fee_bps }.publish(&env);
        Ok(())
    }

    /// Platform fee in basis points taken from each primary sale (issue #104).
    /// Zero when no fee has been set.
    pub fn platform_fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PlatformFeeBps)
            .unwrap_or(0)
    }

    /// Direct, non-marketplace transfer (gift, family member, etc). Publishes
    /// `TicketTransferred` with the old and new owner (issue #115).
    pub fn transfer_ticket(
        env: Env,
        from: Address,
        ticket_id: u64,
        to: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        from.require_auth();
        Self::require_not_paused(&env)?;
        // Issue #128: reject a self-transfer. It would otherwise consume a
        // slot against the per-ticket transfer limit and clear the resale
        // price and any gift claim without moving ownership.
        if from == to {
            return Err(Error::SelfTransfer);
        }
        Self::transfer_owned_ticket(&env, ticket_id, &from, &to)
    }

    /// Direct, non-marketplace batch transfer of tickets to a single recipient.
    /// Bounded by `MAX_BATCH_SIZE`.
    /// The operation is atomic: if any ticket fails validation, all changes
    /// from this call are rolled back.
    pub fn transfer_batch(
        env: Env,
        from: Address,
        ticket_ids: Vec<u64>,
        to: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        from.require_auth();
        Self::require_not_paused(&env)?;
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        // Issue #322: reject a self-transfer. It would otherwise consume a
        // slot against the per-ticket transfer limit and clear the resale
        // price and any gift claim without moving ownership.
        if from == to {
            return Err(Error::SelfTransfer);
        }
        for ticket_id in ticket_ids.iter() {
            let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
            if ticket.owner != from {
                return Err(Error::NotOwner);
            }
            Self::require_not_terminal(&ticket)?;
            Self::ensure_no_auction(&env, ticket_id)?;
            let event = Self::get_event_inner(&env, ticket.event_id)?;
            if Self::transfer_frozen(&env, &event) {
                return Err(Error::TransfersFrozen);
            }
            Self::ensure_transferable(&event)?;
            Self::ensure_transfer_allowed(&env, &ticket, &event)?;
            Self::move_owner_balance(&env, ticket_id, &ticket.owner, &to);
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
        Self::require_not_paused(&env)?;
        if expires_at <= env.ledger().timestamp() {
            return Err(Error::InvalidExpiry);
        }

        let key = DataKey::GiftClaim(ticket_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::GiftClaimAlreadyExists);
        }

        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        Self::require_not_terminal(&ticket)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_transferable(&event)?;
        Self::ensure_no_auction(&env, ticket_id)?;
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
        Self::require_not_paused(&env)?;
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
        Self::require_not_terminal(&ticket)?;

        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if Self::transfer_frozen(&env, &event) {
            return Err(Error::TransfersFrozen);
        }
        Self::ensure_transferable(&event)?;
        Self::ensure_transfer_allowed(&env, &ticket, &event)?;

        Self::move_owner_balance(&env, ticket_id, &ticket.owner, &recipient);
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
        Self::extend_instance_ttl(&env);
        Self::get_ticket(env, ticket_id)
    }

    /// Returns whether a ticket's status permits check-in, without requiring
    /// its owner's address or returning the full ticket record.
    ///
    /// Returns `false` for used, revoked, or resale-listed tickets. Like
    /// `get_ticket`, errors report an uninitialized contract, missing ticket,
    /// or invalid stored lifecycle. This query requires no authorization and
    /// does not renew storage TTL.
    pub fn is_ticket_valid(env: Env, ticket_id: u64) -> Result<bool, Error> {
        Ok(Self::get_ticket(env, ticket_id)?.status == TicketStatus::Valid)
    }

    /// Returns whether a ticket exists, belongs to `owner`, and is valid for
    /// entry. Missing tickets return `false` so scanners can use this as a
    /// single boolean check without handling a contract error.
    pub fn is_valid(env: Env, ticket_id: u64, owner: Address) -> bool {
        match Self::get_ticket(env, ticket_id) {
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
        Self::require_not_paused(&env)?;
        Self::do_check_in(&env, &organizer, ticket_id, None)
    }

    /// Checks a ticket in at a specific zone or gate (issue #80). Fails with
    /// `Error::ZoneNotAllowed` when the ticket's tier is restricted to other
    /// zones; tiers with no zone list may enter anywhere. `signer` is the
    /// organizer or one of the event's check-in signers (issue #89).
    pub fn check_in_at_zone(
        env: Env,
        signer: Address,
        ticket_id: u64,
        zone: String,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        signer.require_auth();
        Self::require_not_paused(&env)?;
        Self::do_check_in(&env, &signer, ticket_id, Some(zone))
    }

    /// Restricts a tier to a set of zones/gates (issue #80). An empty list
    /// removes the restriction. At most `MAX_TIER_ZONES` labels, each
    /// non-empty and at most `MAX_TICKET_LABEL_LEN` bytes.
    pub fn set_tier_zones(
        env: Env,
        organizer: Address,
        event_id: u64,
        tier: String,
        zones: Vec<String>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_organizer(&env, event_id, &organizer)?;
        Self::validate_label(&tier, MAX_TICKET_LABEL_LEN)?;
        if zones.len() > MAX_TIER_ZONES {
            return Err(Error::InvalidZones);
        }
        for zone in zones.iter() {
            if zone.is_empty() || zone.len() > MAX_TICKET_LABEL_LEN {
                return Err(Error::InvalidZones);
            }
        }
        let key = DataKey::TierZones(event_id, tier);
        if zones.is_empty() {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(&key, &zones);
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    /// Zones a tier may enter; empty when the tier is unrestricted (issue #80).
    pub fn tier_zones(env: Env, event_id: u64, tier: String) -> Vec<String> {
        env.storage()
            .persistent()
            .get(&DataKey::TierZones(event_id, tier))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Attaches VIP perks to a tier, e.g. "lounge access" or "free drink"
    /// (issue #68). Perks are informational labels for wallets and gate apps;
    /// they do not change pricing or access. An empty list removes the perks.
    /// At most `MAX_TIER_PERKS` labels, each non-empty and at most
    /// `MAX_PERK_LEN` bytes.
    pub fn set_tier_perks(
        env: Env,
        organizer: Address,
        event_id: u64,
        tier: String,
        perks: Vec<String>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_organizer(&env, event_id, &organizer)?;
        Self::validate_label(&tier, MAX_TICKET_LABEL_LEN)?;
        if perks.len() > MAX_TIER_PERKS {
            return Err(Error::InvalidPerks);
        }
        for perk in perks.iter() {
            if perk.is_empty() || perk.len() > MAX_PERK_LEN {
                return Err(Error::InvalidPerks);
            }
        }
        let key = DataKey::TierPerks(event_id, tier);
        if perks.is_empty() {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(&key, &perks);
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    /// Perks attached to a tier; empty when the tier has none (issue #68).
    pub fn tier_perks(env: Env, event_id: u64, tier: String) -> Vec<String> {
        env.storage()
            .persistent()
            .get(&DataKey::TierPerks(event_id, tier))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Sets one custom metadata entry on an event, e.g. `("venue", "O2
    /// Arena")` (issue #70). Overwrites an existing key. The map is bounded:
    /// adding a new key fails with `Error::EventMetadataFull` once the event
    /// holds `MAX_EVENT_METADATA_ENTRIES` entries. Keys are non-empty and at
    /// most `MAX_EVENT_METADATA_KEY_LEN` bytes; values are non-empty and at
    /// most `MAX_EVENT_METADATA_VALUE_LEN` bytes.
    pub fn set_event_metadata(
        env: Env,
        organizer: Address,
        event_id: u64,
        key: String,
        value: String,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_organizer(&env, event_id, &organizer)?;
        if key.is_empty()
            || key.len() > MAX_EVENT_METADATA_KEY_LEN
            || value.is_empty()
            || value.len() > MAX_EVENT_METADATA_VALUE_LEN
        {
            return Err(Error::InvalidEventMetadata);
        }
        let storage_key = DataKey::EventMetadata(event_id);
        let mut metadata: Map<String, String> = env
            .storage()
            .persistent()
            .get(&storage_key)
            .unwrap_or_else(|| Map::new(&env));
        if !metadata.contains_key(key.clone()) && metadata.len() >= MAX_EVENT_METADATA_ENTRIES {
            return Err(Error::EventMetadataFull);
        }
        metadata.set(key, value);
        env.storage().persistent().set(&storage_key, &metadata);
        env.storage()
            .persistent()
            .extend_ttl(&storage_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Removes one custom metadata entry from an event (issue #70). Removing
    /// a missing key is a no-op; the storage entry is dropped once the map is
    /// empty.
    pub fn remove_event_metadata(
        env: Env,
        organizer: Address,
        event_id: u64,
        key: String,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_organizer(&env, event_id, &organizer)?;
        let storage_key = DataKey::EventMetadata(event_id);
        let Some(mut metadata) = env
            .storage()
            .persistent()
            .get::<_, Map<String, String>>(&storage_key)
        else {
            return Ok(());
        };
        metadata.remove(key);
        if metadata.is_empty() {
            env.storage().persistent().remove(&storage_key);
        } else {
            env.storage().persistent().set(&storage_key, &metadata);
            env.storage()
                .persistent()
                .extend_ttl(&storage_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    /// The event's custom metadata map; empty when none is set (issue #70).
    pub fn event_metadata(env: Env, event_id: u64) -> Map<String, String> {
        env.storage()
            .persistent()
            .get(&DataKey::EventMetadata(event_id))
            .unwrap_or_else(|| Map::new(&env))
    }

    /// Lets `signer` (e.g. a gate device key) check tickets in for the event,
    /// so gates don't need the organizer's master key (issue #89).
    pub fn add_checkin_signer(
        env: Env,
        organizer: Address,
        event_id: u64,
        signer: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_organizer(&env, event_id, &organizer)?;
        let key = DataKey::CheckInSigner(event_id, signer);
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Revokes a delegated check-in signer (issue #89).
    pub fn remove_checkin_signer(
        env: Env,
        organizer: Address,
        event_id: u64,
        signer: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_organizer(&env, event_id, &organizer)?;
        env.storage()
            .persistent()
            .remove(&DataKey::CheckInSigner(event_id, signer));
        Ok(())
    }

    /// Whether `signer` is a delegated check-in signer for the event (issue #89).
    pub fn is_checkin_signer(env: Env, event_id: u64, signer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::CheckInSigner(event_id, signer))
    }

    /// Sets the primary sale window in ledger seconds (issue #94).
    /// `purchase_primary` fails with `SaleNotStarted` before `sale_start` and
    /// `SaleEnded` from `sale_end` on. `None` leaves that side open; passing
    /// `None` for both removes the window.
    pub fn set_event_sale_window(
        env: Env,
        organizer: Address,
        event_id: u64,
        sale_start: Option<u64>,
        sale_end: Option<u64>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_organizer(&env, event_id, &organizer)?;
        if let (Some(start), Some(end)) = (sale_start, sale_end) {
            if end <= start {
                return Err(Error::InvalidSaleWindow);
            }
        }
        let key = DataKey::SaleWindow(event_id);
        if sale_start.is_none() && sale_end.is_none() {
            env.storage().persistent().remove(&key);
        } else {
            env.storage()
                .persistent()
                .set(&key, &(sale_start, sale_end));
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    /// The event's primary sale window `(sale_start, sale_end)` (issue #94).
    pub fn sale_window(env: Env, event_id: u64) -> (Option<u64>, Option<u64>) {
        env.storage()
            .persistent()
            .get(&DataKey::SaleWindow(event_id))
            .unwrap_or((None, None))
    }

    /// Sets a ticket's validity window in ledger seconds, e.g. the days a
    /// multi-day pass covers (issue #72). Check-in fails with
    /// `TicketNotYetValid` before `valid_from` and `TicketValidityEnded` after
    /// `valid_until`; both bounds are inclusive. `None` leaves that side open;
    /// passing `None` for both removes the window. The event's own check-in
    /// window and ticket expiry still apply.
    pub fn set_ticket_validity(
        env: Env,
        organizer: Address,
        ticket_id: u64,
        valid_from: Option<u64>,
        valid_until: Option<u64>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        Self::require_organizer(&env, ticket.event_id, &organizer)?;
        if let (Some(from), Some(until)) = (valid_from, valid_until) {
            if until <= from {
                return Err(Error::InvalidValidityWindow);
            }
        }
        let key = DataKey::TicketValidity(ticket_id);
        if valid_from.is_none() && valid_until.is_none() {
            env.storage().persistent().remove(&key);
        } else {
            env.storage()
                .persistent()
                .set(&key, &(valid_from, valid_until));
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    /// The ticket's validity window `(valid_from, valid_until)` (issue #72).
    pub fn ticket_validity(env: Env, ticket_id: u64) -> (Option<u64>, Option<u64>) {
        env.storage()
            .persistent()
            .get(&DataKey::TicketValidity(ticket_id))
            .unwrap_or((None, None))
    }

    /// Previous owners of a ticket, oldest first, keeping the last
    /// `MAX_OWNER_HISTORY` (issue #82). The current owner is not included.
    pub fn owner_history(env: Env, ticket_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerHistory(ticket_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Marks a batch of tickets as used at the point of entry for group admission.
    /// Only the event's organizer may check tickets in, bounded by `MAX_BATCH_SIZE`.
    /// The operation is atomic: if any ticket fails validation, all changes
    /// from this call are rolled back.
    ///
    /// Tickets listed for resale (`TicketStatus::Resale`) are rejected (issue #132).
    ///
    /// # Errors
    ///
    /// Returns `Error::ResaleListingActive` if any ticket in the batch is currently listed for resale.
    pub fn check_in_batch(env: Env, organizer: Address, ticket_ids: Vec<u64>) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        for ticket_id in ticket_ids.iter() {
            let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
            let event = Self::require_check_in_authority(&env, ticket.event_id, &organizer)?;
            Self::ensure_check_in_window(&env, &event)?;
            Self::ensure_not_expired(&env, &event)?;
            Self::ensure_ticket_validity(&env, ticket_id)?;
            Self::require_not_terminal(&ticket)?;
            if ticket.status == TicketStatus::Resale {
                return Err(Error::ResaleListingActive);
            }
            Self::consume_use(&env, ticket_id, &mut ticket);
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
    /// resold, or checked in again. Does not decrement `Event.tickets_issued`,
    /// which tracks lifetime issuance rather than active supply.
    ///
    /// # Errors
    ///
    /// Returns `Error::Revoked` if the ticket has already been revoked.
    pub fn revoke_ticket(env: Env, organizer: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        Self::require_organizer(&env, ticket.event_id, &organizer)?;
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
        Self::require_not_paused(&env)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        Self::require_organizer(&env, ticket.event_id, &organizer)?;
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
    /// Does not decrement `Event.tickets_issued`.
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
        Self::require_not_paused(&env)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let mut event = Self::require_organizer(&env, ticket.event_id, &organizer)?;
        Self::require_not_terminal(&ticket)?;
        if refund && ticket.original_price > 0 {
            let token_client =
                token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
            let refund_source =
                if event.escrow_enabled && event.escrow_balance >= ticket.original_price {
                    env.current_contract_address()
                } else {
                    organizer.clone()
                };
            token_client.transfer(&refund_source, &ticket.owner, &ticket.original_price);
            if event.escrow_enabled && event.escrow_balance >= ticket.original_price {
                event.escrow_balance -= ticket.original_price;
                let current_escrow = env
                    .storage()
                    .instance()
                    .get::<_, i128>(&DataKey::OutstandingEscrow)
                    .unwrap_or(0);
                env.storage().instance().set(
                    &DataKey::OutstandingEscrow,
                    &(current_escrow - ticket.original_price),
                );
            }
            // The refunded ticket no longer counts against the owner's
            // `max_per_wallet` allowance.
            Self::decrement_wallet_purchase_count(&env, ticket.event_id, &ticket.owner);
        }
        ticket.status = TicketStatus::Revoked;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        if event.escrow_enabled {
            Self::save_event(&env, ticket.event_id, &event);
        }
        Ok(())
    }

    /// Mass revocation of tickets by the event organizer (chargeback, policy violation).
    /// Bounded by `MAX_BATCH_SIZE`. Does not decrement `Event.tickets_issued`.
    /// The operation is atomic: if any ticket fails validation, all changes
    /// from this call are rolled back.
    ///
    /// # Errors
    ///
    /// Returns `Error::Revoked` if any ticket in the batch has already been revoked.
    pub fn revoke_batch(env: Env, organizer: Address, ticket_ids: Vec<u64>) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        if ticket_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if ticket_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        for ticket_id in ticket_ids.iter() {
            let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
            Self::require_organizer(&env, ticket.event_id, &organizer)?;
            if ticket.status == TicketStatus::Revoked {
                return Err(Error::Revoked);
            }
            ticket.status = TicketStatus::Revoked;
            Self::remove_gift_claim(&env, ticket_id);
            Self::save_ticket(&env, ticket_id, &ticket);
        }
        Ok(())
    }

    /// Permanently deletes a used ticket's record once its retention period has
    /// passed (issue #113). The period ends `USED_TICKET_RETENTION_SECONDS`
    /// after the event's `starts_at`. Only the event's organizer may burn, and
    /// only a ticket with `TicketStatus::Used`. Afterwards the ticket reads as
    /// `TicketNotFound`. `Event.tickets_issued` and `total_tickets` still count
    /// it, since both only grow.
    ///
    /// # Errors
    ///
    /// Returns `Error::TicketNotUsed` if the ticket has not been checked in.
    /// Returns `Error::RetentionPeriodNotElapsed` before the retention period ends.
    pub fn burn_used_ticket(env: Env, organizer: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::require_organizer(&env, ticket.event_id, &organizer)?;
        if ticket.status != TicketStatus::Used {
            return Err(Error::TicketNotUsed);
        }
        let burnable_from = event
            .starts_at
            .saturating_add(USED_TICKET_RETENTION_SECONDS);
        if env.ledger().timestamp() < burnable_from {
            return Err(Error::RetentionPeriodNotElapsed);
        }
        Self::debit_owner_balance(&env, &ticket.owner);
        Self::remove_from_owner_index(&env, &ticket.owner, ticket_id);
        env.storage()
            .persistent()
            .remove(&DataKey::Ticket(ticket_id));
        env.storage()
            .persistent()
            .remove(&DataKey::TicketMetadataUri(ticket_id));
        env.storage()
            .persistent()
            .remove(&DataKey::MaxUses(ticket_id));
        env.storage()
            .persistent()
            .remove(&DataKey::UsedCount(ticket_id));
        env.storage()
            .persistent()
            .remove(&DataKey::TicketApproval(ticket_id));
        env.storage()
            .persistent()
            .remove(&DataKey::TicketValidity(ticket_id));
        Self::remove_gift_claim(&env, ticket_id);
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
        Self::require_not_paused(&env)?;
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        Self::require_not_terminal(&ticket)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_transferable(&event)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        Self::ensure_resale_price_in_band(&ticket, &event, price)?;
        Self::ensure_no_auction(&env, ticket_id)?;
        ticket.status = TicketStatus::Resale;
        ticket.resale_price = price;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        TicketListed {
            ticket_id,
            owner,
            price,
        }
        .publish(&env);
        Ok(())
    }

    /// Exposes the resale price cap for a given ticket.
    ///
    /// The cap is computed as `original_price * max_resale_multiplier_bps / 10_000`.
    /// Returns `Error::MathOverflow` if the arithmetic overflows.
    pub fn get_resale_price_cap(env: Env, ticket_id: u64) -> Result<i128, Error> {
        Self::extend_instance_ttl(&env);
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::calculate_resale_cap(ticket.original_price, event.max_resale_multiplier_bps)
    }

    /// Internal helper calculating the resale cap with overflow protection.
    pub fn calculate_resale_cap(
        original_price: i128,
        max_resale_multiplier_bps: u32,
    ) -> Result<i128, Error> {
        let product = original_price
            .checked_mul(max_resale_multiplier_bps as i128)
            .ok_or(Error::MathOverflow)?;
        Ok(product / 10_000)
    }

    /// Cancels a ticket's resale listing, returning its status to valid.
    ///
    /// Resale actions close at the event cutoff, so once the current ledger time
    /// reaches `starts_at - resale_cutoff_seconds`, this rejects with
    /// `Error::ResaleClosed` even when the listing still exists.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotOwner` if the caller is not the ticket's owner.
    /// Returns `Error::NotForResale` if the ticket is not currently listed for resale.
    /// Returns `Error::ResaleClosed` once the cutoff has been reached.
    pub fn cancel_resale(env: Env, owner: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        Self::require_not_paused(&env)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        // An auction with bids is binding until it settles. One without bids
        // can be withdrawn, since no funds are held for it (issue #105).
        if let Some(auction) = env
            .storage()
            .persistent()
            .get::<_, Auction>(&DataKey::Auction(ticket_id))
        {
            if auction.highest_bidder.is_some() {
                return Err(Error::AuctionHasBids);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::Auction(ticket_id));
        }
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::save_ticket(&env, ticket_id, &ticket);
        ListingCancelled { ticket_id, owner }.publish(&env);
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
        Self::require_not_paused(&env)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        // A ticket in an auction is sold by the auction, not at a fixed price (issue #105).
        Self::ensure_no_auction(&env, ticket_id)?;
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
        Self::ensure_transferable(&event)?;
        Self::ensure_transfer_allowed(&env, &ticket, &event)?;
        let token_address = Self::payment_token_for_event(&env, &event)?;

        // --- effects: settle state before any untrusted external call. ---
        // The sale price is captured before the ticket is overwritten, so the
        // published event carries the price the buyer actually paid.
        let price = ticket.resale_price;
        let (royalty, seller_amount) = Self::split_resale_price(&event, ticket.resale_price)?;
        let price = ticket.resale_price;
        let seller = ticket.owner.clone();
        Self::move_owner_balance(&env, ticket_id, &seller, &buyer);
        ticket.owner = buyer.clone();
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);

        // --- interactions: the only untrusted calls, after state is final.
        let token_client = token::Client::new(&env, &token_address);
        if royalty > 0 {
            Self::pay_royalty(&token_client, &buyer, &event, royalty)?;
        }
        if seller_amount > 0 {
            token_client.transfer(&buyer, &seller, &seller_amount);
        }
        TicketResold {
            ticket_id,
            seller,
            buyer,
            price,
            royalty,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns a read-only quote for resale settlement: `(price, royalty, seller_amount)`.
    ///
    /// Matches the calculation in `buy_resale` without modifying any state.
    ///
    /// # Errors
    ///
    /// Returns `Error::TicketNotFound` if the ticket does not exist.
    /// Returns `Error::NotForResale` if the ticket is not listed for resale.
    /// Returns `Error::EventNotFound` if the associated event does not exist.
    /// Returns `Error::MathOverflow` on arithmetic overflow.
    pub fn quote_resale(env: Env, ticket_id: u64) -> Result<(i128, i128, i128), Error> {
        Self::extend_instance_ttl(&env);
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        // An auctioned ticket has no fixed price to quote (issue #105).
        Self::ensure_no_auction(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        let (royalty, seller_amount) = Self::split_resale_price(&event, ticket.resale_price)?;
        Ok((ticket.resale_price, royalty, seller_amount))
    }

    /// Fetches an event by its id. Note that `Event.tickets_issued` is a
    /// monotonic lifetime issuance counter and is not decremented when tickets
    /// are revoked or checked in.
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

    fn bump_ticket_ttl(env: &Env, ticket_id: u64) -> Result<(), Error> {
        let key = DataKey::Ticket(ticket_id);
        if !env.storage().persistent().has(&key) {
            return Err(Error::TicketNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        // The metadata URI lives under its own key, so keep it alive for as
        // long as the ticket it describes (issue #109).
        let uri_key = DataKey::TicketMetadataUri(ticket_id);
        if env.storage().persistent().has(&uri_key) {
            env.storage()
                .persistent()
                .extend_ttl(&uri_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        // A running auction and any operator approval live under their own keys,
        // so renew them with the ticket too (issues #105 and #107).
        let auction_key = DataKey::Auction(ticket_id);
        if env.storage().persistent().has(&auction_key) {
            env.storage()
                .persistent()
                .extend_ttl(&auction_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        let approval_key = DataKey::TicketApproval(ticket_id);
        if env.storage().persistent().has(&approval_key) {
            env.storage()
                .persistent()
                .extend_ttl(&approval_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        // The owner's balance counter is shared across that owner's tickets
        // and only written on ownership changes, so renew it here too. Without
        // this a holder who keeps a ticket for longer than the TTL would see
        // SEP-50 `balance` silently drop to zero (issue #110).
        let owner = Self::get_ticket_inner(env, ticket_id)?.owner;
        // The owner's ticket index (issue #111) is renewed alongside the
        // balance: its length and this ticket's position and slot.
        let count_key = DataKey::OwnerTicketCount(owner.clone());
        if env.storage().persistent().has(&count_key) {
            env.storage()
                .persistent()
                .extend_ttl(&count_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        let pos_key = DataKey::TicketOwnerPos(ticket_id);
        if let Some(pos) = env.storage().persistent().get::<_, u32>(&pos_key) {
            env.storage()
                .persistent()
                .extend_ttl(&pos_key, LEDGER_THRESHOLD, LEDGER_BUMP);
            let slot = DataKey::OwnerTicketAt(owner.clone(), pos);
            if env.storage().persistent().has(&slot) {
                env.storage()
                    .persistent()
                    .extend_ttl(&slot, LEDGER_THRESHOLD, LEDGER_BUMP);
            }
        }
        let balance_key = DataKey::OwnerBalance(owner);
        if env.storage().persistent().has(&balance_key) {
            env.storage()
                .persistent()
                .extend_ttl(&balance_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    /// Returns one page of the ticket ids `owner` holds, in any lifecycle
    /// status (issue #111). `start` is a zero-based position in the owner's
    /// list, `limit` is clamped to `MAX_PAGE_SIZE`, and an empty list comes
    /// back once `start` reaches the end. The order is not chronological: a
    /// transfer or burn moves the last entry into the freed slot.
    pub fn tickets_of(env: Env, owner: Address, start: u32, limit: u32) -> Vec<u64> {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerTicketCount(owner.clone()))
            .unwrap_or(0);
        let end = start.saturating_add(limit.min(MAX_PAGE_SIZE)).min(count);
        let mut ticket_ids = Vec::new(&env);
        for index in start..end {
            let slot = DataKey::OwnerTicketAt(owner.clone(), index);
            if let Some(ticket_id) = env.storage().persistent().get::<_, u64>(&slot) {
                ticket_ids.push_back(ticket_id);
            }
        }
        ticket_ids
    }

    /// Returns the number of events registered by `organizer`.
    pub fn get_organizer_events(env: Env, organizer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::OrganizerEvents(organizer))
            .unwrap_or(0)
    }

    /// Returns one page of the event ids registered by `organizer` (issue
    /// #112). `start` is a zero-based position in the organizer's event list.
    /// `limit` is clamped to `MAX_PAGE_SIZE`, and an empty list comes back once
    /// `start` reaches the end.
    ///
    /// Events created before this index existed have no slot here, so they are
    /// skipped; `get_organizer_events` still counts them.
    pub fn list_organizer_events(env: Env, organizer: Address, start: u32, limit: u32) -> Vec<u64> {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::OrganizerEvents(organizer.clone()))
            .unwrap_or(0);
        let end = start.saturating_add(limit.min(MAX_PAGE_SIZE)).min(count);
        let mut event_ids = Vec::new(&env);
        for index in start..end {
            let slot = DataKey::OrganizerEventAt(organizer.clone(), index);
            if let Some(event_id) = env.storage().persistent().get::<_, u64>(&slot) {
                event_ids.push_back(event_id);
            }
        }
        event_ids
    }

    /// SEP-50 `owner_of`: the address that currently owns a ticket (issue
    /// #110). The ticket id is the SEP-50 `TokenID`.
    ///
    /// # Errors
    ///
    /// Returns `Error::TicketNotFound` when no ticket with `token_id` exists.
    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, Error> {
        Ok(Self::get_ticket(env, token_id)?.owner)
    }

    /// SEP-50 `balance`: how many tickets `owner` holds, in any lifecycle
    /// status (issue #110). The standard names this function `balance`, not
    /// `balance_of`. Revoked and used tickets still count, since they are
    /// still owned.
    pub fn balance(env: Env, owner: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerBalance(owner))
            .unwrap_or(0)
    }

    /// Total number of tickets ever issued across all events (issue #114).
    ///
    /// This is the next ticket id, so it only grows. Tickets that were later
    /// revoked, checked in or burned still count. It is the global counterpart
    /// of the per-event `Event.tickets_issued`.
    pub fn total_tickets(env: Env) -> u64 {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::NextTicketId)
            .unwrap_or(0)
    }

    /// SEP-50 `token_uri`: the metadata URI set when the ticket was issued
    /// (issues #109 and #110).
    ///
    /// # Errors
    ///
    /// Returns `Error::TicketNotFound` when no ticket with `token_id` exists,
    /// and `Error::MetadataUriNotSet` when it was issued without a URI.
    pub fn token_uri(env: Env, token_id: u64) -> Result<String, Error> {
        Self::get_ticket(env.clone(), token_id)?;
        env.storage()
            .persistent()
            .get(&DataKey::TicketMetadataUri(token_id))
            .ok_or(Error::MetadataUriNotSet)
    }

    /// Starts a timed auction for an owned, valid ticket (issue #105). Bids are
    /// accepted until `ends_at`. The ticket stays in the resale state while the
    /// auction runs, so it cannot be transferred, gifted, or bought at a fixed
    /// price. `reserve_price` is the lowest accepted bid and must sit inside the
    /// event's resale price band. `ends_at` must be in the future and no later
    /// than the resale cutoff, so the auction settles while resale is open.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidPrice` for a non-positive reserve, `Error::NotOwner`
    /// for a non-owner, `Error::AuctionActive` when an auction already runs,
    /// `Error::ResaleListingActive` when the ticket has a fixed-price listing,
    /// `Error::ResaleClosed` once the cutoff has passed, and
    /// `Error::InvalidAuctionEnd` for an `ends_at` outside the allowed window.
    pub fn start_auction(
        env: Env,
        owner: Address,
        ticket_id: u64,
        reserve_price: i128,
        ends_at: u64,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        if reserve_price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        Self::require_not_terminal(&ticket)?;
        if Self::has_auction(&env, ticket_id) {
            return Err(Error::AuctionActive);
        }
        if ticket.status == TicketStatus::Resale {
            return Err(Error::ResaleListingActive);
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_transferable(&event)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        Self::ensure_transfer_allowed(&env, &ticket, &event)?;
        Self::ensure_resale_price_in_band(&ticket, &event, reserve_price)?;
        let cutoff = event.starts_at.saturating_sub(event.resale_cutoff_seconds);
        if ends_at <= env.ledger().timestamp() || ends_at > cutoff {
            return Err(Error::InvalidAuctionEnd);
        }

        Self::remove_gift_claim(&env, ticket_id);
        ticket.status = TicketStatus::Resale;
        ticket.resale_price = 0;
        Self::save_ticket(&env, ticket_id, &ticket);
        let auction = Auction {
            seller: owner.clone(),
            reserve_price,
            ends_at,
            highest_bidder: None,
            highest_bid: 0,
        };
        Self::save_auction(&env, ticket_id, &auction);
        AuctionStarted {
            ticket_id,
            seller: owner,
            reserve_price,
            ends_at,
        }
        .publish(&env);
        Ok(())
    }

    /// Places a bid on a running auction (issue #105). A bid must reach the
    /// reserve, beat the current highest bid, and stay within the resale price
    /// cap. The bid is escrowed in the contract, and the previous highest bidder
    /// is refunded as soon as they are outbid. Bidding closes at `ends_at`.
    ///
    /// # Errors
    ///
    /// Returns `Error::AuctionNotFound` when the ticket has no auction,
    /// `Error::AuctionEnded` at or after `ends_at`, `Error::SelfPurchase` for the
    /// seller, `Error::BidTooLow` when the bid does not beat the highest bid or
    /// the reserve, and `Error::ResalePriceExceedsCap` above the cap.
    pub fn place_bid(env: Env, bidder: Address, ticket_id: u64, amount: i128) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        bidder.require_auth();
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        Self::require_not_terminal(&ticket)?;
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        let mut auction = Self::get_auction_inner(&env, ticket_id)?;
        if env.ledger().timestamp() >= auction.ends_at {
            return Err(Error::AuctionEnded);
        }
        if ticket.owner == bidder {
            return Err(Error::SelfPurchase);
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_not_cancelled(&event)?;
        let cap =
            Self::calculate_resale_cap(ticket.original_price, event.max_resale_multiplier_bps)?;
        if amount > cap {
            return Err(Error::ResalePriceExceedsCap);
        }
        if amount < auction.reserve_price || amount <= auction.highest_bid {
            return Err(Error::BidTooLow);
        }
        let token_address = Self::payment_token_for_event(&env, &event)?;

        // --- effects: record the new high bid before any tokens move. ---
        let outbid = auction
            .highest_bidder
            .clone()
            .map(|previous| (previous, auction.highest_bid));
        let refunded = outbid.as_ref().map_or(0, |(_, bid)| *bid);
        auction.highest_bidder = Some(bidder.clone());
        auction.highest_bid = amount;
        Self::save_auction(&env, ticket_id, &auction);
        Self::adjust_resale_funds(&env, amount - refunded);

        // --- interactions: take the new bid, then refund the one it displaced. ---
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&bidder, env.current_contract_address(), &amount);
        if let Some((previous, previous_bid)) = outbid {
            token_client.transfer(&env.current_contract_address(), &previous, &previous_bid);
        }
        BidPlaced {
            ticket_id,
            bidder,
            amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Settles an auction once bidding has closed (issue #105). Anyone may call
    /// this after `ends_at`. The highest bidder receives the ticket, and the
    /// contract pays the royalty and the seller's share out of the bid it holds.
    /// With no bids the ticket goes back to valid. If the ticket was revoked
    /// while the auction ran, the highest bid is refunded and the ticket stays
    /// revoked.
    ///
    /// # Errors
    ///
    /// Returns `Error::AuctionNotFound` when the ticket has no auction, and
    /// `Error::AuctionNotEnded` before `ends_at`.
    pub fn settle_auction(env: Env, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let auction = Self::get_auction_inner(&env, ticket_id)?;
        if env.ledger().timestamp() < auction.ends_at {
            return Err(Error::AuctionNotEnded);
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        let token_address = Self::payment_token_for_event(&env, &event)?;
        let bid = auction.highest_bid;
        let listed = ticket.status == TicketStatus::Resale;

        // --- effects: close the auction and settle the ticket before any transfer. ---
        env.storage()
            .persistent()
            .remove(&DataKey::Auction(ticket_id));
        Self::adjust_resale_funds(&env, -bid);
        match &auction.highest_bidder {
            Some(winner) if listed => {
                let seller = ticket.owner.clone();
                let (royalty, seller_amount) = Self::split_resale_price(&event, bid)?;
                Self::move_owner_balance(&env, ticket_id, &seller, winner);
                ticket.owner = winner.clone();
                ticket.transfers += 1;
                ticket.status = TicketStatus::Valid;
                ticket.resale_price = 0;
                Self::remove_gift_claim(&env, ticket_id);
                Self::save_ticket(&env, ticket_id, &ticket);

                // --- interactions: pay out from the bid the contract holds. ---
                let token_client = token::Client::new(&env, &token_address);
                let contract = env.current_contract_address();
                if royalty > 0 {
                    Self::pay_royalty(&token_client, &contract, &event, royalty)?;
                }
                if seller_amount > 0 {
                    token_client.transfer(&contract, &seller, &seller_amount);
                }
                AuctionSettled {
                    ticket_id,
                    buyer: winner.clone(),
                    amount: bid,
                }
                .publish(&env);
            }
            Some(bidder) => {
                // Revoked while bidding ran: no ticket moves, so return the bid.
                token::Client::new(&env, &token_address).transfer(
                    &env.current_contract_address(),
                    bidder,
                    &bid,
                );
            }
            None if listed => {
                ticket.status = TicketStatus::Valid;
                ticket.resale_price = 0;
                Self::save_ticket(&env, ticket_id, &ticket);
            }
            None => {}
        }
        Ok(())
    }

    /// Reads the open auction for a ticket (issue #105).
    ///
    /// # Errors
    ///
    /// Returns `Error::AuctionNotFound` when the ticket has no open auction.
    pub fn get_auction(env: Env, ticket_id: u64) -> Result<Auction, Error> {
        Self::get_auction_inner(&env, ticket_id)
    }

    /// Makes an offer on a listed ticket (issue #106). The offer may sit below
    /// or above the asking price, but it must stay inside the resale price band,
    /// so it is capped at `max_resale_multiplier_bps` like any listing. The
    /// amount is escrowed until the owner accepts it or it is cancelled. A buyer
    /// holds at most one open offer per ticket.
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidPrice` for a non-positive amount, `Error::NotForResale`
    /// when the ticket is not listed, `Error::SelfPurchase` for the owner,
    /// `Error::OfferAlreadyExists` when the buyer already has an offer,
    /// `Error::ResaleClosed` once the cutoff has passed, and the price band
    /// errors `Error::ResalePriceExceedsCap` and `Error::ResalePriceBelowFloor`.
    pub fn make_offer(env: Env, buyer: Address, ticket_id: u64, amount: i128) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        buyer.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidPrice);
        }
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        Self::ensure_no_auction(&env, ticket_id)?;
        if ticket.owner == buyer {
            return Err(Error::SelfPurchase);
        }
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_transferable(&event)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        Self::ensure_resale_price_in_band(&ticket, &event, amount)?;
        let key = DataKey::Offer(ticket_id, buyer.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::OfferAlreadyExists);
        }
        let token_address = Self::payment_token_for_event(&env, &event)?;

        // --- effects: record the offer before taking its funds. ---
        let offer = Offer {
            seller: ticket.owner.clone(),
            amount,
            token: token_address.clone(),
        };
        env.storage().persistent().set(&key, &offer);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Self::adjust_resale_funds(&env, amount);

        // --- interactions ---
        token::Client::new(&env, &token_address).transfer(
            &buyer,
            env.current_contract_address(),
            &amount,
        );
        OfferMade {
            ticket_id,
            buyer,
            amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Accepts an open offer and sells the ticket to its buyer (issue #106). The
    /// owner the offer was made to must still hold the ticket, and it must still
    /// be listed. The royalty and the seller's share are paid out of the escrowed
    /// offer, following the same rules as `buy_resale`. Other open offers on the
    /// ticket stay escrowed, but they can no longer be accepted, and their buyers
    /// can withdraw them.
    ///
    /// # Errors
    ///
    /// Returns `Error::OfferNotFound` when the buyer has no open offer,
    /// `Error::NotOwner` when the caller is not the current owner who received
    /// the offer, `Error::NotForResale` when the ticket is no longer listed, and
    /// `Error::ResaleClosed` once the cutoff has passed.
    pub fn accept_offer(
        env: Env,
        owner: Address,
        ticket_id: u64,
        buyer: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        let key = DataKey::Offer(ticket_id, buyer.clone());
        let offer: Offer = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::OfferNotFound)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner || offer.seller != owner {
            return Err(Error::NotOwner);
        }
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        Self::ensure_no_auction(&env, ticket_id)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        if Self::resale_closed(&env, &event) {
            return Err(Error::ResaleClosed);
        }
        Self::ensure_transferable(&event)?;
        Self::ensure_transfer_allowed(&env, &ticket, &event)?;
        let (royalty, seller_amount) = Self::split_resale_price(&event, offer.amount)?;

        // --- effects: settle state before any untrusted external call. ---
        env.storage().persistent().remove(&key);
        Self::adjust_resale_funds(&env, -offer.amount);
        Self::move_owner_balance(&env, ticket_id, &owner, &buyer);
        ticket.owner = buyer.clone();
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);

        // --- interactions: pay out from the escrowed offer. ---
        let token_client = token::Client::new(&env, &offer.token);
        let contract = env.current_contract_address();
        if royalty > 0 {
            Self::pay_royalty(&token_client, &contract, &event, royalty)?;
        }
        if seller_amount > 0 {
            token_client.transfer(&contract, &owner, &seller_amount);
        }
        OfferAccepted {
            ticket_id,
            buyer,
            amount: offer.amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Cancels an open offer and returns the escrowed funds to the buyer (issue
    /// #106). The owner the offer was made to can reject it, and the buyer can
    /// withdraw their own offer. Neither path requires the ticket to still be
    /// listed.
    ///
    /// # Errors
    ///
    /// Returns `Error::OfferNotFound` when the buyer has no open offer, and
    /// `Error::NotOwner` when the caller is neither the buyer nor the offer's seller.
    pub fn cancel_offer(
        env: Env,
        caller: Address,
        ticket_id: u64,
        buyer: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        caller.require_auth();
        let key = DataKey::Offer(ticket_id, buyer.clone());
        let offer: Offer = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::OfferNotFound)?;
        if caller != buyer && caller != offer.seller {
            return Err(Error::NotOwner);
        }

        // --- effects, then refund. ---
        env.storage().persistent().remove(&key);
        Self::adjust_resale_funds(&env, -offer.amount);
        token::Client::new(&env, &offer.token).transfer(
            &env.current_contract_address(),
            &buyer,
            &offer.amount,
        );
        OfferCancelled {
            ticket_id,
            buyer,
            amount: offer.amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Reads an open offer on a ticket (issue #106).
    ///
    /// # Errors
    ///
    /// Returns `Error::OfferNotFound` when the buyer has no open offer.
    pub fn get_offer(env: Env, ticket_id: u64, buyer: Address) -> Result<Offer, Error> {
        let offer: Offer = env
            .storage()
            .persistent()
            .get(&DataKey::Offer(ticket_id, buyer))
            .ok_or(Error::OfferNotFound)?;
        Ok(offer)
    }

    /// Approves `operator` to move one ticket on the owner's behalf with
    /// `transfer_from` (issue #107). A ticket has at most one approved operator,
    /// so a new approval replaces the old one. The approval is cleared whenever
    /// the ticket changes owner, so a new holder never inherits it.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotOwner` for a non-owner, `Error::SelfTransfer` when the
    /// operator is the owner, and `Error::Revoked` or `Error::AlreadyUsed` for a
    /// ticket that can no longer move.
    pub fn approve(
        env: Env,
        owner: Address,
        ticket_id: u64,
        operator: Address,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        Self::require_not_terminal(&ticket)?;
        if operator == owner {
            return Err(Error::SelfTransfer);
        }
        let key = DataKey::TicketApproval(ticket_id);
        env.storage().persistent().set(&key, &operator);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Removes the operator approval on a ticket (issue #107). Idempotent.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotOwner` when the caller does not own the ticket.
    pub fn revoke_approval(env: Env, owner: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        owner.require_auth();
        let ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        env.storage()
            .persistent()
            .remove(&DataKey::TicketApproval(ticket_id));
        Ok(())
    }

    /// The operator approved to move a ticket, or `None` when no approval is
    /// set (issue #107).
    pub fn get_approved(env: Env, ticket_id: u64) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::TicketApproval(ticket_id))
    }

    /// Moves a ticket from `from` to `to` on behalf of its approved operator
    /// `spender` (issue #107). The argument order follows SEP-50. The move follows
    /// the same rules as `transfer_ticket`, and the approval is cleared once the
    /// ticket changes owner.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotApproved` when `spender` is not the approved operator,
    /// and `Error::NotOwner` when `from` does not own the ticket.
    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        ticket_id: u64,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        spender.require_auth();
        if from == to {
            return Err(Error::SelfTransfer);
        }
        let approved: Option<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::TicketApproval(ticket_id));
        if approved != Some(spender) {
            return Err(Error::NotApproved);
        }
        Self::transfer_owned_ticket(&env, ticket_id, &from, &to)
    }

    /// Loads an event and verifies its organizer after the caller has
    /// required authorization at the entry point.
    /// Shared body of `check_in` / `check_in_at_zone`. Callers have already
    /// required `signer`'s auth.
    fn do_check_in(
        env: &Env,
        signer: &Address,
        ticket_id: u64,
        zone: Option<String>,
    ) -> Result<(), Error> {
        let mut ticket = Self::get_ticket_inner(env, ticket_id)?;
        let event = Self::require_check_in_authority(env, ticket.event_id, signer)?;
        Self::ensure_check_in_window(env, &event)?;
        Self::ensure_not_expired(env, &event)?;
        Self::ensure_ticket_validity(env, ticket_id)?;
        Self::require_not_terminal(&ticket)?;
        if ticket.status == TicketStatus::Resale {
            return Err(Error::ResaleListingActive);
        }
        if let Some(zone) = zone {
            Self::ensure_zone_allowed(env, ticket.event_id, &ticket.tier, &zone)?;
        }
        Self::consume_use(env, ticket_id, &mut ticket);
        ticket.resale_price = 0;
        Self::remove_gift_claim(env, ticket_id);
        Self::save_ticket(env, ticket_id, &ticket);
        TicketCheckedIn {
            ticket_id,
            organizer: signer.clone(),
        }
        .publish(env);
        Ok(())
    }

    /// The organizer, or a delegated check-in signer for the event (issue
    /// #89). Anyone else gets `Error::NotOrganizer`.
    fn require_check_in_authority(
        env: &Env,
        event_id: u64,
        signer: &Address,
    ) -> Result<Event, Error> {
        let event = Self::get_event_inner(env, event_id)?;
        if event.organizer == *signer
            || env
                .storage()
                .persistent()
                .has(&DataKey::CheckInSigner(event_id, signer.clone()))
        {
            return Ok(event);
        }
        Err(Error::NotOrganizer)
    }

    /// Tiers with no zone list may enter anywhere (issue #80).
    fn ensure_zone_allowed(
        env: &Env,
        event_id: u64,
        tier: &String,
        zone: &String,
    ) -> Result<(), Error> {
        let zones: Option<Vec<String>> = env
            .storage()
            .persistent()
            .get(&DataKey::TierZones(event_id, tier.clone()));
        match zones {
            Some(zones) if !zones.contains(zone) => Err(Error::ZoneNotAllowed),
            _ => Ok(()),
        }
    }

    /// Enforces the optional primary sale window (issue #94).
    fn ensure_sale_open(env: &Env, event_id: u64) -> Result<(), Error> {
        let window: Option<(Option<u64>, Option<u64>)> = env
            .storage()
            .persistent()
            .get(&DataKey::SaleWindow(event_id));
        let Some((start, end)) = window else {
            return Ok(());
        };
        let now = env.ledger().timestamp();
        if let Some(start) = start {
            if now < start {
                return Err(Error::SaleNotStarted);
            }
        }
        if let Some(end) = end {
            if now >= end {
                return Err(Error::SaleEnded);
            }
        }
        Ok(())
    }

    /// Appends `previous` to a ticket's bounded owner history (issue #82),
    /// dropping the oldest entry once `MAX_OWNER_HISTORY` is reached.
    fn record_previous_owner(env: &Env, ticket_id: u64, previous: &Address) {
        let key = DataKey::OwnerHistory(ticket_id);
        let mut history: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        while history.len() >= MAX_OWNER_HISTORY {
            history.pop_front();
        }
        history.push_back(previous.clone());
        env.storage().persistent().set(&key, &history);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn require_organizer(env: &Env, event_id: u64, organizer: &Address) -> Result<Event, Error> {
        let event = Self::get_event_inner(env, event_id)?;
        if event.organizer != *organizer {
            return Err(Error::NotOrganizer);
        }
        Ok(event)
    }

    /// Rejects operations that cannot act on used or revoked tickets.
    /// Resale tickets remain eligible, subject to each operation's rules.
    fn require_not_terminal(ticket: &Ticket) -> Result<(), Error> {
        match ticket.status {
            TicketStatus::Used => Err(Error::AlreadyUsed),
            TicketStatus::Revoked => Err(Error::Revoked),
            TicketStatus::Refunded => Err(Error::AlreadyRefunded),
            _ => Ok(()),
        }
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
        let stored: StoredTicket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)?;
        StoredTicket::unpack(stored)
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

    /// Appends `event_id` to the organizer's paginated event list at the next
    /// free index, then advances the organizer's event count (issue #112).
    fn index_organizer_event(env: &Env, organizer: &Address, event_id: u64) {
        let key = DataKey::OrganizerEvents(organizer.clone());
        let index: u32 = env.storage().persistent().get(&key).unwrap_or(0u32);
        let slot = DataKey::OrganizerEventAt(organizer.clone(), index);
        env.storage().persistent().set(&slot, &event_id);
        env.storage()
            .persistent()
            .extend_ttl(&slot, LEDGER_THRESHOLD, LEDGER_BUMP);
        env.storage()
            .persistent()
            .set(&key, &index.saturating_add(1));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Rejects every movement of a ticket on a soulbound event (issue #108).
    fn ensure_transferable(event: &Event) -> Result<(), Error> {
        if event.transferable {
            Ok(())
        } else {
            Err(Error::NotTransferable)
        }
    }

    /// Records one ticket leaving `from` and one arriving at `to` (issue
    /// #110). Callers pass the owner as it stood before the change.
    fn move_owner_balance(env: &Env, ticket_id: u64, from: &Address, to: &Address) {
        Self::record_previous_owner(env, ticket_id, from);
        Self::debit_owner_balance(env, from);
        Self::credit_owner_balance(env, to);
        Self::remove_from_owner_index(env, from, ticket_id);
        Self::add_to_owner_index(env, to, ticket_id);
        // An operator approval belongs to the owner who granted it, so it ends
        // with that ownership (issue #107).
        env.storage()
            .persistent()
            .remove(&DataKey::TicketApproval(ticket_id));
    }

    /// Appends a ticket to its owner's index at the next free slot (issue #111).
    fn add_to_owner_index(env: &Env, owner: &Address, ticket_id: u64) {
        let count_key = DataKey::OwnerTicketCount(owner.clone());
        let index: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        let slot = DataKey::OwnerTicketAt(owner.clone(), index);
        let pos_key = DataKey::TicketOwnerPos(ticket_id);
        env.storage().persistent().set(&slot, &ticket_id);
        env.storage().persistent().set(&pos_key, &index);
        env.storage()
            .persistent()
            .set(&count_key, &index.saturating_add(1));
        for key in [slot, pos_key, count_key] {
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
    }

    /// Removes a ticket from its owner's index by moving the owner's last
    /// entry into the freed slot, so the list stays dense (issue #111). A
    /// ticket with no position entry is skipped, since that entry expired.
    fn remove_from_owner_index(env: &Env, owner: &Address, ticket_id: u64) {
        let pos_key = DataKey::TicketOwnerPos(ticket_id);
        let pos: u32 = match env.storage().persistent().get(&pos_key) {
            Some(pos) => pos,
            None => return,
        };
        let count_key = DataKey::OwnerTicketCount(owner.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if count == 0 {
            return;
        }
        let last = count - 1;
        if pos != last {
            let last_slot = DataKey::OwnerTicketAt(owner.clone(), last);
            if let Some(moved_id) = env.storage().persistent().get::<_, u64>(&last_slot) {
                let slot = DataKey::OwnerTicketAt(owner.clone(), pos);
                env.storage().persistent().set(&slot, &moved_id);
                env.storage()
                    .persistent()
                    .extend_ttl(&slot, LEDGER_THRESHOLD, LEDGER_BUMP);
                let moved_pos = DataKey::TicketOwnerPos(moved_id);
                env.storage().persistent().set(&moved_pos, &pos);
                env.storage()
                    .persistent()
                    .extend_ttl(&moved_pos, LEDGER_THRESHOLD, LEDGER_BUMP);
            }
        }
        env.storage()
            .persistent()
            .remove(&DataKey::OwnerTicketAt(owner.clone(), last));
        env.storage().persistent().remove(&pos_key);
        env.storage().persistent().set(&count_key, &last);
        env.storage()
            .persistent()
            .extend_ttl(&count_key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn credit_owner_balance(env: &Env, owner: &Address) {
        let key = DataKey::OwnerBalance(owner.clone());
        let current: u64 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_add(1));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn debit_owner_balance(env: &Env, owner: &Address) {
        let key = DataKey::OwnerBalance(owner.clone());
        let current: u64 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_sub(1));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Stores a ticket's metadata URI under its own key (issue #109).
    fn store_metadata_uri(env: &Env, ticket_id: u64, uri: &String) {
        let key = DataKey::TicketMetadataUri(ticket_id);
        env.storage().persistent().set(&key, uri);
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
            .set(&key, &StoredTicket::pack(ticket));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Moves a ticket to `to` after checking that `from` owns it. Shared by
    /// `transfer_ticket` and `transfer_from` so both apply the same rules
    /// (issues #107 and #115).
    fn transfer_owned_ticket(
        env: &Env,
        ticket_id: u64,
        from: &Address,
        to: &Address,
    ) -> Result<(), Error> {
        let mut ticket = Self::get_ticket_inner(env, ticket_id)?;
        if ticket.owner != *from {
            return Err(Error::NotOwner);
        }
        Self::require_not_terminal(&ticket)?;
        let event = Self::get_event_inner(env, ticket.event_id)?;
        Self::ensure_transferable(&event)?;
        Self::ensure_transfer_allowed(env, &ticket, &event)?;
        if Self::transfer_frozen(env, &event) {
            return Err(Error::TransfersFrozen);
        }
        Self::ensure_no_auction(env, ticket_id)?;
        Self::move_owner_balance(env, ticket_id, &ticket.owner, to);
        ticket.owner = to.clone();
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::remove_gift_claim(env, ticket_id);
        Self::save_ticket(env, ticket_id, &ticket);
        TicketTransferred {
            ticket_id,
            from: from.clone(),
            to: to.clone(),
        }
        .publish(env);
        Ok(())
    }

    /// Reads an auction record, or fails with `AuctionNotFound` (issue #105).
    fn get_auction_inner(env: &Env, ticket_id: u64) -> Result<Auction, Error> {
        let auction: Auction = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(ticket_id))
            .ok_or(Error::AuctionNotFound)?;
        Ok(auction)
    }

    fn has_auction(env: &Env, ticket_id: u64) -> bool {
        env.storage().persistent().has(&DataKey::Auction(ticket_id))
    }

    /// Rejects an operation that would move or sell a ticket while its auction
    /// is running (issue #105).
    fn ensure_no_auction(env: &Env, ticket_id: u64) -> Result<(), Error> {
        if Self::has_auction(env, ticket_id) {
            return Err(Error::AuctionActive);
        }
        Ok(())
    }

    /// Writes an auction and refreshes its TTL (issue #105).
    fn save_auction(env: &Env, ticket_id: u64, auction: &Auction) {
        let key = DataKey::Auction(ticket_id);
        env.storage().persistent().set(&key, auction);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Total funds the contract holds for resale bids and offers (issues #105, #106).
    fn resale_funds_outstanding(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::OutstandingResaleFunds)
            .unwrap_or(0)
    }

    /// Moves the contract's resale funds total by `delta` (issues #105, #106).
    fn adjust_resale_funds(env: &Env, delta: i128) {
        let total = Self::resale_funds_outstanding(env) + delta;
        env.storage()
            .instance()
            .set(&DataKey::OutstandingResaleFunds, &total);
    }

    /// Platform fee owed on a primary sale of `price`, rounded down (issue #104).
    fn platform_fee_on(env: &Env, price: i128) -> Result<i128, Error> {
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PlatformFeeBps)
            .unwrap_or(0);
        if fee_bps == 0 {
            return Ok(0);
        }
        let product = price
            .checked_mul(fee_bps as i128)
            .ok_or(Error::MathOverflow)?;
        Ok(product / BPS_DENOMINATOR as i128)
    }

    /// Splits a resale price into `(royalty, seller_amount)` using the event's
    /// royalty rate. The seller takes the remainder, so the two always sum to
    /// `price`.
    fn split_resale_price(event: &Event, price: i128) -> Result<(i128, i128), Error> {
        let royalty_product = price
            .checked_mul(event.royalty_bps as i128)
            .ok_or(Error::MathOverflow)?;
        let royalty = royalty_product / BPS_DENOMINATOR as i128;
        Ok((royalty, price - royalty))
    }

    /// Checks a resale-market price against the event's cap and floor (issues
    /// #105, #106). Listings, bids and offers all use the same band.
    fn ensure_resale_price_in_band(
        ticket: &Ticket,
        event: &Event,
        price: i128,
    ) -> Result<(), Error> {
        let cap =
            Self::calculate_resale_cap(ticket.original_price, event.max_resale_multiplier_bps)?;
        if price > cap {
            return Err(Error::ResalePriceExceedsCap);
        }
        if let Some(floor_bps) = event.min_resale_multiplier_bps {
            let floor_product = ticket
                .original_price
                .checked_mul(floor_bps as i128)
                .ok_or(Error::MathOverflow)?;
            let floor = floor_product / 10_000;
            if price < floor {
                return Err(Error::ResalePriceBelowFloor);
            }
        }
        Ok(())
    }

    fn remove_gift_claim(env: &Env, ticket_id: u64) {
        env.storage()
            .persistent()
            .remove(&DataKey::GiftClaim(ticket_id));
    }

    fn max_uses_of(env: &Env, ticket_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MaxUses(ticket_id))
            .unwrap_or(1)
    }

    fn used_count_of(env: &Env, ticket_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::UsedCount(ticket_id))
            .unwrap_or(0)
    }

    fn store_used_count(env: &Env, ticket_id: u64, count: u32) {
        let key = DataKey::UsedCount(ticket_id);
        env.storage().persistent().set(&key, &count);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    /// Records one entry against a ticket's allowed uses (issue #74). The
    /// ticket becomes `Used` only once its uses are exhausted, so multi-use
    /// passes stay `Valid` between entries.
    fn consume_use(env: &Env, ticket_id: u64, ticket: &mut Ticket) {
        let used = Self::used_count_of(env, ticket_id) + 1;
        Self::store_used_count(env, ticket_id, used);
        ticket.status = if used >= Self::max_uses_of(env, ticket_id) {
            TicketStatus::Used
        } else {
            TicketStatus::Valid
        };
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

    fn ensure_transfer_allowed(env: &Env, ticket: &Ticket, event: &Event) -> Result<(), Error> {
        if let Some(max_transfers) = event.max_transfers_per_ticket {
            if ticket.transfers >= max_transfers {
                return Err(Error::TransferLimitExceeded);
            }
        }
        Self::ensure_not_expired(env, event)
    }

    /// Rejects a primary mint once the event has reached its `max_tickets`
    /// cap (issue #92). `amount` is how many tickets the caller is about to
    /// mint, so a batch lottery is checked as a whole.
    fn ensure_capacity(event: &Event, amount: u64) -> Result<(), Error> {
        match event.max_tickets {
            Some(max) if event.tickets_issued.saturating_add(amount) > max => Err(Error::SoldOut),
            _ => Ok(()),
        }
    }

    /// Rejects every new issuance, sale or listing once the organizer has
    /// cancelled the event (issue #91).
    fn ensure_not_cancelled(event: &Event) -> Result<(), Error> {
        if event.cancelled {
            return Err(Error::EventCancelled);
        }
        Ok(())
    }

    /// Number of tickets issued so far in one of an event's tiers (issue #93).
    fn tier_issued(env: &Env, event_id: u64, tier: &String) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::TierIssued(event_id, tier.clone()))
            .unwrap_or(0)
    }

    /// Counts `amount` more tickets against a tier and rejects the mint when
    /// the tier's cap would be exceeded (issue #93). Every mint path calls
    /// this, so tiers are tracked even when no cap is set.
    fn reserve_tier_capacity(
        env: &Env,
        event_id: u64,
        tier: &String,
        amount: u64,
    ) -> Result<(), Error> {
        let next = Self::tier_issued(env, event_id, tier).saturating_add(amount);
        let cap_key = DataKey::TierCapacity(event_id, tier.clone());
        if let Some(cap) = env.storage().persistent().get::<_, u64>(&cap_key) {
            if next > cap {
                return Err(Error::TierSoldOut);
            }
        }
        let issued_key = DataKey::TierIssued(event_id, tier.clone());
        env.storage().persistent().set(&issued_key, &next);
        env.storage()
            .persistent()
            .extend_ttl(&issued_key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Pays a resale royalty to the organizer, or splits it across the event's
    /// `royalty_splits` when set (issue #90). Each share is `royalty * bps /
    /// royalty_bps`, and the last recipient takes the rounding remainder so the
    /// payouts always add up to `royalty`.
    fn pay_royalty(
        token_client: &token::Client,
        buyer: &Address,
        event: &Event,
        royalty: i128,
    ) -> Result<(), Error> {
        let splits = match &event.royalty_splits {
            Some(splits) => splits,
            None => {
                token_client.transfer(buyer, &event.organizer, &royalty);
                return Ok(());
            }
        };
        let last = splits.len().saturating_sub(1);
        let mut paid: i128 = 0;
        for (i, (recipient, bps)) in splits.iter().enumerate() {
            let share = if i as u32 == last {
                royalty - paid
            } else {
                royalty
                    .checked_mul(bps as i128)
                    .ok_or(Error::MathOverflow)?
                    / event.royalty_bps as i128
            };
            paid += share;
            if share > 0 {
                token_client.transfer(buyer, &recipient, &share);
            }
        }
        Ok(())
    }

    /// Rejects check-in before the window opens or after it closes (issue
    /// #95). The window is inclusive of `ends_at`.
    fn ensure_check_in_window(env: &Env, event: &Event) -> Result<(), Error> {
        let now = env.ledger().timestamp();
        if let Some(opens_before) = event.check_in_opens_seconds {
            if now < event.starts_at.saturating_sub(opens_before) {
                return Err(Error::CheckInWindowClosed);
            }
        }
        if let Some(ends_at) = event.ends_at {
            if now > ends_at {
                return Err(Error::CheckInWindowClosed);
            }
        }
        Ok(())
    }

    /// Rejects check-in and transfer once a ticket has expired (issue #76).
    /// A ticket is expired from the instant `ticket_expires_at` is reached.
    fn ensure_not_expired(env: &Env, event: &Event) -> Result<(), Error> {
        match event.ticket_expires_at {
            Some(expires_at) if env.ledger().timestamp() >= expires_at => Err(Error::TicketExpired),
            _ => Ok(()),
        }
    }

    /// Rejects check-in outside a ticket's own validity window (issue #72).
    /// Tickets without a window pass.
    fn ensure_ticket_validity(env: &Env, ticket_id: u64) -> Result<(), Error> {
        let window: Option<(Option<u64>, Option<u64>)> = env
            .storage()
            .persistent()
            .get(&DataKey::TicketValidity(ticket_id));
        let Some((valid_from, valid_until)) = window else {
            return Ok(());
        };
        let now = env.ledger().timestamp();
        if valid_from.is_some_and(|from| now < from) {
            return Err(Error::TicketNotYetValid);
        }
        if valid_until.is_some_and(|until| now > until) {
            return Err(Error::TicketValidityEnded);
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
            owner: to.clone(),
            tier: tier.clone(),
            seat,
            status: TicketStatus::Valid,
            original_price: price,
            resale_price: 0,
            transfers: 0,
            purchased_at: env.ledger().timestamp(),
        };
        Self::save_ticket(env, ticket_id, &ticket);
        Self::credit_owner_balance(env, &ticket.owner);
        Self::add_to_owner_index(env, &ticket.owner, ticket_id);
        env.storage()
            .instance()
            .set(&DataKey::NextTicketId, &(ticket_id + 1));
        TicketIssued {
            ticket_id,
            event_id,
            owner: to,
            tier,
        }
        .publish(env);
        ticket_id
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn require_not_paused(env: &Env) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }
        Ok(())
    }

    pub fn refund_primary(env: Env, buyer: Address, ticket_id: u64) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        buyer.require_auth();
        Self::require_not_paused(&env)?;
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        if ticket.owner != buyer {
            return Err(Error::NotOwner);
        }
        if ticket.status == TicketStatus::Refunded {
            return Err(Error::AlreadyRefunded);
        }
        Self::require_not_terminal(&ticket)?;
        let mut event = Self::get_event_inner(&env, ticket.event_id)?;
        if let Some(refund_window) = event.refund_window_seconds {
            let refund_deadline = ticket.purchased_at.saturating_add(refund_window);
            if env.ledger().timestamp() > refund_deadline {
                return Err(Error::RefundWindowExpired);
            }
        } else {
            return Err(Error::RefundWindowExpired);
        }
        ticket.status = TicketStatus::Refunded;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        if ticket.original_price > 0 {
            let token_client =
                token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
            let refund_source =
                if event.escrow_enabled && event.escrow_balance >= ticket.original_price {
                    env.current_contract_address()
                } else {
                    event.organizer.clone()
                };
            token_client.transfer(&refund_source, &buyer, &ticket.original_price);
            if event.escrow_enabled && event.escrow_balance >= ticket.original_price {
                event.escrow_balance -= ticket.original_price;
                let current_escrow = env
                    .storage()
                    .instance()
                    .get::<_, i128>(&DataKey::OutstandingEscrow)
                    .unwrap_or(0);
                env.storage().instance().set(
                    &DataKey::OutstandingEscrow,
                    &(current_escrow - ticket.original_price),
                );
                Self::save_event(&env, ticket.event_id, &event);
            }
        }
        Self::decrement_wallet_purchase_count(&env, ticket.event_id, &buyer);
        Ok(())
    }

    pub fn add_to_allowlist(
        env: Env,
        organizer: Address,
        event_id: u64,
        members: Vec<Address>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_organizer(&env, event_id, &organizer)?;
        for member in members.iter() {
            let key = DataKey::AllowlistMember(event_id, member);
            env.storage().persistent().set(&key, &true);
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
        Ok(())
    }

    pub fn remove_from_allowlist(
        env: Env,
        organizer: Address,
        event_id: u64,
        members: Vec<Address>,
    ) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        organizer.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_organizer(&env, event_id, &organizer)?;
        for member in members.iter() {
            env.storage()
                .persistent()
                .remove(&DataKey::AllowlistMember(event_id, member));
        }
        Ok(())
    }

    pub fn is_on_allowlist(env: Env, event_id: u64, member: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::AllowlistMember(event_id, member))
    }

    fn enforce_presale_rules(
        env: &Env,
        event_id: u64,
        buyer: &Address,
        event: &Event,
    ) -> Result<(), Error> {
        if let (Some(presale_starts), Some(presale_ends)) =
            (event.presale_starts_at, event.presale_ends_at)
        {
            let now = env.ledger().timestamp();
            let in_presale = now >= presale_starts && now < presale_ends;
            let on_allowlist = env
                .storage()
                .persistent()
                .has(&DataKey::AllowlistMember(event_id, buyer.clone()));
            if in_presale && !on_allowlist {
                return Err(Error::NotOnAllowlist);
            }
        }
        Ok(())
    }

    fn enforce_wallet_limit(
        env: &Env,
        event_id: u64,
        buyer: &Address,
        event: &Event,
    ) -> Result<(), Error> {
        if let Some(max_per_wallet) = event.max_per_wallet {
            let count = Self::get_wallet_purchase_count(env, event_id, buyer);
            if count >= max_per_wallet {
                return Err(Error::PurchaseLimitExceeded);
            }
        }
        Ok(())
    }

    fn get_wallet_purchase_count(env: &Env, event_id: u64, wallet: &Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::WalletPurchaseCount(event_id, wallet.clone()))
            .unwrap_or(0)
    }

    fn increment_wallet_purchase_count(env: &Env, event_id: u64, wallet: &Address) {
        let key = DataKey::WalletPurchaseCount(event_id, wallet.clone());
        let count = env.storage().persistent().get(&key).unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&key, &count.saturating_add(1));
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn decrement_wallet_purchase_count(env: &Env, event_id: u64, wallet: &Address) {
        let key = DataKey::WalletPurchaseCount(event_id, wallet.clone());
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        if count > 0 {
            env.storage()
                .persistent()
                .set(&key, &count.saturating_sub(1));
            env.storage()
                .persistent()
                .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        }
    }
}

#[cfg(test)]
mod test;
