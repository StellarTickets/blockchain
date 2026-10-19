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
    CATEGORY_OTHER, CATEGORY_SPORTS, MAX_BATCH_SIZE, MAX_CATEGORY_LEN, MAX_METADATA_URI_LEN,
    MAX_NAME_LEN, MAX_OWNER_HISTORY, MAX_PAGE_SIZE, MAX_TICKET_LABEL_LEN, MAX_TIER_ZONES,
    PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS, USED_TICKET_RETENTION_SECONDS,
};
use constants::{LEDGER_BUMP, LEDGER_THRESHOLD};
pub use error::Error;
pub use events::{
    ContractInitialized, ContractUpgraded, EventCancelled, OrganizerApproved, OrganizerRevoked,
    PaymentTokenChanged, PaymentTokenProposed, PurchaseThrottleUpdated, TicketCheckedIn,
    TicketIssued, TicketTransferred,
};
pub use types::{
    Category, DataKey, Event, EventParams, GiftClaim, PendingPaymentToken, StoredTicket, Ticket,
    TicketRecipient, TicketStatus,
};

use soroban_sdk::{
    contract, contractimpl, token, Address, Bytes, BytesN, ContractExecutable, Env, String, Vec,
};

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
        // Issue #126: bound tier/seat so they cannot inflate per-ticket
        // storage cost and rent. Validated before the throttle and the tier
        // price lookup so an oversized label is rejected on its own terms.
        Self::validate_ticket_labels(&tier, &seat)?;
        Self::enforce_purchase_throttle(&env, &buyer)?;
        let mut event = Self::get_event_inner(&env, event_id)?;
        Self::ensure_not_cancelled(&event)?;
        Self::ensure_sale_open(&env, event_id)?;
        Self::ensure_capacity(&event, 1)?;
        let price = Self::tier_price(&env, event_id, &tier)?;
        Self::reserve_tier_capacity(&env, event_id, &tier, 1)?;
        let token_client = token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
        if price > 0 {
            if event.escrow_enabled {
                token_client.transfer(&buyer, env.current_contract_address(), &price);
                event.escrow_balance += price;
                let current_escrow = env
                    .storage()
                    .instance()
                    .get::<_, i128>(&DataKey::OutstandingEscrow)
                    .unwrap_or(0);
                env.storage()
                    .instance()
                    .set(&DataKey::OutstandingEscrow, &(current_escrow + price));
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
        Self::require_not_terminal(&ticket)?;
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        Self::ensure_transferable(&event)?;
        Self::ensure_transfer_allowed(&env, &ticket, &event)?;
        if Self::transfer_frozen(&env, &event) {
            return Err(Error::TransfersFrozen);
        }
        Self::move_owner_balance(&env, ticket_id, &ticket.owner, &to);
        ticket.owner = to.clone();
        ticket.transfers += 1;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
        TicketTransferred {
            ticket_id,
            from,
            to,
        }
        .publish(&env);
        Ok(())
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
            Self::require_not_terminal(&ticket)?;
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
        let mut ticket = Self::get_ticket_inner(&env, ticket_id)?;
        let mut event = Self::require_organizer(&env, ticket.event_id, &organizer)?;
        Self::require_not_terminal(&ticket)?;
        if refund && ticket.original_price > 0 {
            let token_client =
                token::Client::new(&env, &Self::payment_token_for_event(&env, &event)?);
            let refund_source = if event.escrow_enabled && event.escrow_balance >= ticket.original_price {
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
                env.storage()
                    .instance()
                    .set(&DataKey::OutstandingEscrow, &(current_escrow - ticket.original_price));
            }
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
        let burnable_from = event.starts_at.saturating_add(USED_TICKET_RETENTION_SECONDS);
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
        let cap = Self::calculate_resale_cap(ticket.original_price, event.max_resale_multiplier_bps)?;
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
        ticket.status = TicketStatus::Resale;
        ticket.resale_price = price;
        Self::remove_gift_claim(&env, ticket_id);
        Self::save_ticket(&env, ticket_id, &ticket);
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
        Self::ensure_transferable(&event)?;
        Self::ensure_transfer_allowed(&env, &ticket, &event)?;
        let token_address = Self::payment_token_for_event(&env, &event)?;

        // --- effects: settle state before any untrusted external call. ---
        let royalty_product = ticket
            .resale_price
            .checked_mul(event.royalty_bps as i128)
            .ok_or(Error::MathOverflow)?;
        let royalty = royalty_product / BPS_DENOMINATOR as i128;
        let seller_amount = ticket.resale_price - royalty;
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
        let event = Self::get_event_inner(&env, ticket.event_id)?;
        let royalty_product = ticket
            .resale_price
            .checked_mul(event.royalty_bps as i128)
            .ok_or(Error::MathOverflow)?;
        let royalty = royalty_product / BPS_DENOMINATOR as i128;
        let seller_amount = ticket.resale_price - royalty;
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
                royalty.checked_mul(bps as i128).ok_or(Error::MathOverflow)?
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
            Some(expires_at) if env.ledger().timestamp() >= expires_at => {
                Err(Error::TicketExpired)
            }
            _ => Ok(()),
        }
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

    /// Version of the contract build, taken from the crate's `Cargo.toml` at
    /// compile time (issue #102). Consumers read it to tell which build is
    /// deployed before relying on newer entry points.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    /// Replaces this contract's WASM with `new_wasm_hash` (issue #101). The
    /// new code must already be on the ledger. Admin only.
    ///
    /// Only the code changes. Stored keys and value layouts stay as they are,
    /// so a new build must still read existing state or ship a migration. See
    /// [docs/UPGRADES.md](../../docs/UPGRADES.md) for the rules.
    ///
    /// # Errors
    ///
    /// Returns `Error::NotAdmin` when `admin` is not the stored admin.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        Self::extend_instance_ttl(&env);
        Self::require_admin(&env, &admin)?;
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(new_wasm_hash.clone()));
        ContractUpgraded {
            admin,
            new_wasm_hash,
        }
        .publish(&env);
        Ok(())
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
}

#[cfg(test)]
mod test;
