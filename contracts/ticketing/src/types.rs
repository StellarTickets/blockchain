//! Contract storage types shared across the ticketing contract.

use soroban_sdk::{contracttype, Address, BytesN, String};

/// Lifecycle status of a ticket.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TicketStatus {
    /// Usable for entry and transfer.
    Valid,
    /// Checked in at the gate; no longer usable.
    Used,
    /// Voided by the organizer; permanently unusable.
    Revoked,
    /// Listed on the resale marketplace.
    Resale,
}

/// An event, route, or showing registered by an organizer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    /// Address of the event organizer, the only account allowed to issue
    /// tickets, check them in, revoke them, and enable escrow.
    pub organizer: Address,
    /// Display name of the event.
    pub name: String,
    /// Category such as "concert", "flight", "sports", "conference", etc.
    /// Kept as free text metadata rather than a fixed enum so new industries
    /// don't require a contract migration.
    pub category: String,
    /// Basis points cap on resale price relative to original sale price
    /// (e.g. 12000 = 120%). Anti-scalping enforcement.
    pub max_resale_multiplier_bps: u32,
    /// Optional floor on resale price relative to original price.
    pub min_resale_multiplier_bps: Option<u32>,
    /// Optional maximum number of ownership transfers for tickets in this event.
    pub max_transfers_per_ticket: Option<u32>,
    /// Basis points of every resale price paid to the organizer as royalty.
    pub royalty_bps: u32,
    /// Number of tickets issued for the event so far.
    pub tickets_issued: u64,
    /// Ledger timestamp at which the event starts.
    pub starts_at: u64,
    /// Seconds before `starts_at` after which direct transfers are frozen.
    pub transfer_freeze_seconds: u64,
    /// Seconds before `starts_at` after which resale listings are closed.
    pub resale_cutoff_seconds: u64,
    /// When true, primary sale proceeds are held by the contract instead of
    /// paid to the organizer immediately, and can only be released once the
    /// ledger sequence reaches `escrow_release_ledger`.
    pub escrow_enabled: bool,
    /// Ledger sequence after which escrowed proceeds may be released.
    /// Ignored when `escrow_enabled` is false.
    pub escrow_release_ledger: u32,
    /// Primary sale proceeds currently held in escrow for this event.
    pub escrow_balance: i128,
    /// Per-event accepted payment token (issue #235). `None` means the
    /// event settles in the contract-wide payment token set at
    /// initialization. Lockable only while no tickets have been issued so
    /// existing sales stay denominated in the token they were paid in.
    pub payment_token: Option<Address>,
}

/// A ticket minted for an event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ticket {
    /// Id of the event this ticket admits entry to.
    pub event_id: u64,
    /// Current owner of the ticket.
    pub owner: Address,
    /// Ticket tier name, e.g. "VIP" or "GA".
    pub tier: String,
    /// Assigned seat, or "unassigned" for general admission.
    pub seat: String,
    /// Current lifecycle status of the ticket.
    pub status: TicketStatus,
    /// Price paid at primary sale; used for resale caps and refunds.
    pub original_price: i128,
    /// Asking price while the ticket is listed for resale; 0 otherwise.
    pub resale_price: i128,
    /// Number of ownership transfers completed for this ticket.
    pub transfers: u32,
}

/// A pending gift-claim link for a ticket.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GiftClaim {
    /// Owner the ticket must still belong to when the claim is accepted.
    pub from: Address,
    /// SHA-256 digest of the claim secret; the preimage is shared off-chain.
    pub secret_hash: BytesN<32>,
    /// Ledger timestamp after which the claim can no longer be accepted.
    pub expires_at: u64,
}

/// A payment token change that has been proposed but not yet applied.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPaymentToken {
    /// Token contract address proposed as the new payment token.
    pub token: Address,
    /// Ledger sequence after which the change may be applied.
    pub apply_after_ledger: u32,
}

/// Storage keys for all contract state.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Contract admin address.
    Admin,
    /// Contract-wide payment token address.
    PaymentToken,
    /// Decimals of the active payment token, cached for client display.
    TokenDecimals,
    /// Timelocked payment token change awaiting application.
    PendingPaymentToken,
    /// Event record, keyed by event id.
    Event(u64),
    /// Ticket record, keyed by ticket id.
    Ticket(u64),
    /// Pending gift claim, keyed by ticket id.
    GiftClaim(u64),
    /// Monotonic counter used to allocate new ticket ids.
    NextTicketId,
    /// Ledger sequence of a buyer's most recent primary purchase.
    LastPurchaseLedger(Address),
    /// Minimum ledger spacing enforced between a buyer's primary purchases.
    MinPurchaseSpacing,
}
