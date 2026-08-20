#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env, String,
};

#[contractevent]
#[derive(Clone, Debug)]
pub struct TicketIssued {
    #[topic]
    pub ticket_id: u64,
    pub event_id: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TicketCheckedIn {
    #[topic]
    pub ticket_id: u64,
    pub organizer: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TicketStatus {
    Valid,
    Used,
    Revoked,
    Resale,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Event {
    pub organizer: Address,
    pub name: String,
    /// Category such as "concert", "flight", "sports", "conference", etc.
    /// Kept as free text metadata rather than a fixed enum so new industries
    /// don't require a contract migration.
    pub category: String,
    /// Basis points cap on resale price relative to original sale price
    /// (e.g. 12000 = 120%). Anti-scalping enforcement.
    pub max_resale_multiplier_bps: u32,
    /// Basis points of every resale price paid to the organizer as royalty.
    pub royalty_bps: u32,
    pub tickets_issued: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Ticket {
    pub event_id: u64,
    pub owner: Address,
    pub tier: String,
    pub seat: String,
    pub status: TicketStatus,
    pub original_price: i128,
    pub resale_price: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PaymentToken,
    Event(u64),
    Ticket(u64),
    NextTicketId,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    EventNotFound = 3,
    EventAlreadyExists = 4,
    TicketNotFound = 5,
    NotOrganizer = 6,
    NotOwner = 7,
    AlreadyUsed = 8,
    Revoked = 9,
    NotForResale = 10,
    ResalePriceExceedsCap = 11,
    InvalidPrice = 12,
    InvalidRoyalty = 13,
}

// 31 days * 24h * 3600s / 5s per ledger = 535_680 exactly; 535_679 was one
// ledger short of the documented window.
const LEDGER_BUMP: u32 = 535_680; // ~31 days at 5s/ledger, matches other Soroban tooling defaults
const LEDGER_THRESHOLD: u32 = 500_000;

#[contract]
pub struct TicketingContract;

#[contractimpl]
impl TicketingContract {
    /// One-time setup. `payment_token` is the Stellar Asset Contract (or any
    /// SEP-41 token) used for on-chain primary sales and resale settlement.
    pub fn initialize(env: Env, admin: Address, payment_token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PaymentToken, &payment_token);
        env.storage().instance().set(&DataKey::NextTicketId, &0u64);
        env.storage()
            .instance()
            .extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
    }

    /// Registers a new event/route/showing under an organizer. `event_id` is
    /// chosen by the caller's backend (e.g. a ULID cast to u64) so it can be
    /// correlated with the off-chain event record.
    pub fn create_event(
        env: Env,
        organizer: Address,
        event_id: u64,
        name: String,
        category: String,
        max_resale_multiplier_bps: u32,
        royalty_bps: u32,
    ) -> Result<(), Error> {
        organizer.require_auth();
        if royalty_bps > 10_000 {
            return Err(Error::InvalidRoyalty);
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
            royalty_bps,
            tickets_issued: 0,
        };
        env.storage().persistent().set(&key, &event);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
        Ok(())
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
        organizer.require_auth();
        if price < 0 {
            return Err(Error::InvalidPrice);
        }
        let mut event = Self::get_event(&env, event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        let ticket_id = Self::mint(&env, event_id, to, tier, seat, price);
        event.tickets_issued += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Event(event_id), &event);
        Ok(ticket_id)
    }

    /// Fully on-chain primary sale: buyer pays the organizer directly in
    /// `payment_token`, then the ticket is minted to the buyer atomically.
    pub fn purchase_primary(
        env: Env,
        buyer: Address,
        event_id: u64,
        tier: String,
        seat: String,
        price: i128,
    ) -> Result<u64, Error> {
        buyer.require_auth();
        if price < 0 {
            return Err(Error::InvalidPrice);
        }
        let mut event = Self::get_event(&env, event_id)?;
        let token_client = token::Client::new(&env, &Self::payment_token(&env)?);
        if price > 0 {
            token_client.transfer(&buyer, &event.organizer, &price);
        }
        let ticket_id = Self::mint(&env, event_id, buyer, tier, seat, price);
        event.tickets_issued += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Event(event_id), &event);
        Ok(ticket_id)
    }

    /// Direct, non-marketplace transfer (gift, family member, etc).
    pub fn transfer_ticket(
        env: Env,
        from: Address,
        ticket_id: u64,
        to: Address,
    ) -> Result<(), Error> {
        from.require_auth();
        let mut ticket = Self::get_ticket(&env, ticket_id)?;
        if ticket.owner != from {
            return Err(Error::NotOwner);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }
        ticket.owner = to;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Read-only on-chain verification — the core fraud-prevention primitive.
    /// Any scanner/app can call this without authentication to confirm a
    /// ticket's current owner and status before admitting entry.
    pub fn verify_ticket(env: Env, ticket_id: u64) -> Result<Ticket, Error> {
        Self::get_ticket(&env, ticket_id)
    }

    /// Marks a ticket as used at the point of entry. Only the event's
    /// organizer (or their delegated gate device, via a shared Soroban
    /// signer) may check a ticket in, and only once.
    pub fn check_in(env: Env, organizer: Address, ticket_id: u64) -> Result<(), Error> {
        organizer.require_auth();
        let mut ticket = Self::get_ticket(&env, ticket_id)?;
        let event = Self::get_event(&env, ticket.event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }
        ticket.status = TicketStatus::Used;
        Self::save_ticket(&env, ticket_id, &ticket);
        TicketCheckedIn {
            ticket_id,
            organizer,
        }
        .publish(&env);
        Ok(())
    }

    /// Fraud prevention: organizer voids a ticket (chargeback, counterfeit
    /// report, policy violation). Revoked tickets can never be transferred,
    /// resold, or checked in again.
    pub fn revoke_ticket(env: Env, organizer: Address, ticket_id: u64) -> Result<(), Error> {
        organizer.require_auth();
        let mut ticket = Self::get_ticket(&env, ticket_id)?;
        let event = Self::get_event(&env, ticket.event_id)?;
        if event.organizer != organizer {
            return Err(Error::NotOrganizer);
        }
        ticket.status = TicketStatus::Revoked;
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    /// Lists an owned, valid ticket on the resale marketplace. The price is
    /// capped at the event's `max_resale_multiplier_bps` of the original
    /// sale price to curb scalping.
    pub fn list_for_resale(
        env: Env,
        owner: Address,
        ticket_id: u64,
        price: i128,
    ) -> Result<(), Error> {
        owner.require_auth();
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let mut ticket = Self::get_ticket(&env, ticket_id)?;
        if ticket.owner != owner {
            return Err(Error::NotOwner);
        }
        match ticket.status {
            TicketStatus::Used => return Err(Error::AlreadyUsed),
            TicketStatus::Revoked => return Err(Error::Revoked),
            _ => {}
        }
        let event = Self::get_event(&env, ticket.event_id)?;
        let cap = ticket.original_price * event.max_resale_multiplier_bps as i128 / 10_000;
        if price > cap {
            return Err(Error::ResalePriceExceedsCap);
        }
        ticket.status = TicketStatus::Resale;
        ticket.resale_price = price;
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    pub fn cancel_resale(env: Env, owner: Address, ticket_id: u64) -> Result<(), Error> {
        owner.require_auth();
        let mut ticket = Self::get_ticket(&env, ticket_id)?;
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

    /// Buys a resale-listed ticket. Payment is settled atomically on-chain:
    /// the organizer's royalty cut is paid first, the remainder to the
    /// seller, then ownership transfers to the buyer.
    pub fn buy_resale(env: Env, buyer: Address, ticket_id: u64) -> Result<(), Error> {
        buyer.require_auth();
        let mut ticket = Self::get_ticket(&env, ticket_id)?;
        if ticket.status != TicketStatus::Resale {
            return Err(Error::NotForResale);
        }
        let event = Self::get_event(&env, ticket.event_id)?;
        let token_client = token::Client::new(&env, &Self::payment_token(&env)?);
        let royalty = ticket.resale_price * event.royalty_bps as i128 / 10_000;
        let seller_amount = ticket.resale_price - royalty;
        if royalty > 0 {
            token_client.transfer(&buyer, &event.organizer, &royalty);
        }
        if seller_amount > 0 {
            token_client.transfer(&buyer, &ticket.owner, &seller_amount);
        }
        ticket.owner = buyer;
        ticket.status = TicketStatus::Valid;
        ticket.resale_price = 0;
        Self::save_ticket(&env, ticket_id, &ticket);
        Ok(())
    }

    pub fn get_event(env: &Env, event_id: u64) -> Result<Event, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Event(event_id))
            .ok_or(Error::EventNotFound)
    }

    pub fn get_ticket(env: &Env, ticket_id: u64) -> Result<Ticket, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)
    }

    fn save_ticket(env: &Env, ticket_id: u64, ticket: &Ticket) {
        let key = DataKey::Ticket(ticket_id);
        env.storage().persistent().set(&key, ticket);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }

    fn payment_token(env: &Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::PaymentToken)
            .ok_or(Error::NotInitialized)
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
}

#[cfg(test)]
mod test;
