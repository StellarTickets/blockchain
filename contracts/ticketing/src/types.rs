//! Contract storage types shared across the ticketing contract.
#![allow(missing_docs)]

use crate::Error;
use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

/// Optional common category labels for indexers and client applications.
///
/// `Event.category` intentionally remains a `String` so deployments can add
/// industry-specific labels without a contract migration. Use `Other` when a
/// value does not fit the common vocabulary.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Category {
    Concert,
    Flight,
    Sports,
    Festival,
    Conference,
    Bus,
    Cinema,
    Museum,
    TouristAttraction,
    PublicTransport,
    University,
    Corporate,
    Other,
}

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
    /// Refunded and voided.
    Refunded,
}

/// Configuration parameters for registering an event via `create_event`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventParams {
    /// Display name of the event.
    pub name: String,
    /// Category such as "concert", "flight", "sports", "conference", etc.
    pub category: String,
    /// Basis points cap on resale price relative to original sale price
    /// (e.g. 12000 = 120%). Anti-scalping enforcement.
    pub max_resale_multiplier_bps: u32,
    /// Basis points of every resale price paid to the organizer as royalty.
    pub royalty_bps: u32,
    /// Ledger timestamp at which the event starts.
    pub starts_at: u64,
    /// Seconds before `starts_at` after which direct transfers are frozen.
    pub transfer_freeze_seconds: u64,
    /// Seconds before `starts_at` after which resale listings are closed.
    pub resale_cutoff_seconds: u64,
    /// Optional max tickets per wallet for primary sales.
    pub max_per_wallet: Option<u32>,
    /// Optional refund window in seconds after purchase.
    pub refund_window_seconds: Option<u64>,
    /// Optional presale start timestamp.
    pub presale_starts_at: Option<u64>,
    /// Optional presale end timestamp.
    pub presale_ends_at: Option<u64>,
}

/// An event registered by an organizer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    /// Address of the event organizer, the only account allowed to issue
    /// tickets, check them in, revoke them, and enable escrow.
    pub organizer: Address,
    /// Display name of the event.
    pub name: String,
    /// Category such as "concert", "flight", "sports", "conference", etc.
    /// Source of truth: the backend enforces the shared industry list in
    /// [docs/INDUSTRIES.md](../../docs/INDUSTRIES.md) and the `Category` enum/
    /// `CATEGORY_*` constants exported by this contract. The storage field is a
    /// `String` for ABI compatibility, but callers must use the canonical
    /// backend vocabulary rather than inventing new labels.
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
    /// Cumulative number of tickets ever minted for the event (`issue_ticket`,
    /// `purchase_primary`, `allocate_lottery`). Monotonic — never decremented
    /// when a ticket is revoked or checked in, so revoked tickets still count
    /// toward this total. Do not read this field as active supply.
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
    /// When false the event is soulbound: tickets cannot be transferred,
    /// gifted, listed for resale or bought on the resale market (issue #108).
    /// Defaults to true. Settable only before tickets are issued.
    pub transferable: bool,
    /// Optional max tickets per wallet for primary sales.
    pub max_per_wallet: Option<u32>,
    /// Optional refund window in seconds after purchase.
    pub refund_window_seconds: Option<u64>,
    /// Optional presale start timestamp.
    pub presale_starts_at: Option<u64>,
    /// Optional presale end timestamp.
    pub presale_ends_at: Option<u64>,
    /// Maximum number of tickets the event may ever issue (issue #92). `None`
    /// means uncapped. Compared against `tickets_issued`, so revoked tickets
    /// still count toward it.
    pub max_tickets: Option<u64>,
    /// Seconds before `starts_at` at which check-in opens (issue #95). `None`
    /// means check-in is open from the start of the event.
    pub check_in_opens_seconds: Option<u64>,
    /// Ledger timestamp after which check-in closes (issue #95). `None` means
    /// check-in never closes.
    pub ends_at: Option<u64>,
    /// Ledger timestamp after which tickets can no longer be checked in or
    /// transferred (issue #76). `None` means tickets never expire.
    pub ticket_expires_at: Option<u64>,
    /// How many times a checked-in ticket may be passed back out for re-entry
    /// (issue #78). `0` disables re-entry for the event.
    pub max_reentries: u32,
    /// True once the organizer has cancelled the event (issue #91). A cancelled
    /// event cannot issue, sell or list tickets.
    pub cancelled: bool,
    /// Optional split of `royalty_bps` between several recipients (issue #90).
    /// Each entry is `(recipient, bps)` and the entries sum to `royalty_bps`.
    /// `None` pays the whole royalty to the organizer.
    pub royalty_splits: Option<Vec<(Address, u32)>>,
}

/// One mint request in a batch issuance (issue #88).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TicketRecipient {
    /// Account that receives the ticket.
    pub to: Address,
    /// Ticket tier name, e.g. "VIP" or "GA".
    pub tier: String,
    /// Assigned seat, or "unassigned" for general admission.
    pub seat: String,
    /// Price paid at primary sale; used for resale caps and refunds.
    pub price: i128,
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
    /// Ledger timestamp when this ticket was purchased.
    pub purchased_at: u64,
}

/// Compact persistent representation for a ticket.
///
/// The public `Ticket` ABI stays unchanged for clients, while storage packs
/// status and transfer count into one lifecycle word to reduce the persistent
/// footprint of every ticket record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTicket {
    /// Id of the event this ticket admits entry to.
    pub event_id: u64,
    /// Current owner of the ticket.
    pub owner: Address,
    /// Ticket tier name, e.g. "VIP" or "GA".
    pub tier: String,
    /// Assigned seat, or "unassigned" for general admission.
    pub seat: String,
    /// Packed lifecycle state: low byte is `TicketStatus`, high bits are transfers.
    pub lifecycle: u64,
    /// Price paid at primary sale; used for resale caps and refunds.
    pub original_price: i128,
    /// Asking price while the ticket is listed for resale; 0 otherwise.
    pub resale_price: i128,
    /// Ledger timestamp when this ticket was purchased.
    pub purchased_at: u64,
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

/// A timed auction for a ticket, started with `start_auction` (issue #105).
/// The highest bid is held by the contract until the auction settles, or
/// until its bidder is outbid and refunded.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Auction {
    /// Owner who started the auction. The ticket must still belong to them
    /// when the auction settles.
    pub seller: Address,
    /// Lowest accepted bid, and the minimum opening bid.
    pub reserve_price: i128,
    /// Ledger timestamp at which bidding closes and the auction can settle.
    pub ends_at: u64,
    /// Bidder currently holding the highest bid, or `None` before the first bid.
    pub highest_bidder: Option<Address>,
    /// Amount of the highest bid, held by the contract. Zero before the first bid.
    pub highest_bid: i128,
}

/// A buyer's offer on a listed ticket, held in escrow until it is accepted or
/// cancelled (issue #106).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Offer {
    /// Owner the offer was made to. Only that owner can accept it, so an offer
    /// cannot be settled after the ticket has changed hands.
    pub seller: Address,
    /// Offered price, held by the contract.
    pub amount: i128,
    /// Payment token the offer was escrowed in. Stored so a cancellation can
    /// refund it even if the ticket record no longer exists.
    pub token: Address,
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
///
/// # Index naming convention
///
/// Index-style variants follow three shapes so a reader can tell how a key
/// is scanned without looking up its call sites:
///
/// 1. **Ordered list slots** — `<Subject>At(subject_id, index)` returns the
///    id stored at a zero-based position, with a companion
///    `<Subject>Count(subject_id)` or `<Subject>Events(subject_id)` giving
///    the list length. Examples: `OwnerTicketAt` / `OwnerTicketCount`,
///    `OrganizerEventAt` / `OrganizerEvents`.
/// 2. **Reverse position lookup** — `<Subject>Pos(subject_id)` maps an
///    element back to its slot in the owner's list so removal is constant
///    time. Example: `TicketOwnerPos`.
/// 3. **Composite per-entity keys** — `<Subject>(owner_id, child_id)`
///    (or `<Subject>(event_id, label)` for tier- and policy-scoped data)
///    stores one entry per (parent, child) pair without a list walk.
///    Examples: `TierPrice(event_id, tier)`, `AllowlistMember(event_id,
///    member)`, `Offer(ticket_id, buyer)`.
///
/// When adding a new index, pick the shape that matches how it is read —
/// paged iteration uses (1), O(1) removal from a list uses (2), and
/// membership or per-child configuration uses (3). Do not rename existing
/// variants: the XDR encoding of each variant name is part of the storage
/// ABI. The full storage reference lives in `docs/STORAGE_LAYOUT.md`.
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
    /// Number of tickets issued for an event. Kept separate from `Event` so
    /// minting does not rewrite the complete event record.
    TicketsIssued(u64),
    /// Primary sale price for one of an event's tiers, keyed by event id and
    /// tier name (issue #127). Stored alongside the event record rather than
    /// inside it so changing a tier price never rewrites the whole `Event`
    /// struct, which also keeps already-stored event records readable.
    TierPrice(u64, String),
    /// Number of events registered by an organizer.
    OrganizerEvents(Address),
    /// Ledger sequence of a buyer's most recent primary purchase.
    LastPurchaseLedger(Address),
    /// Minimum ledger spacing enforced between a buyer's primary purchases.
    MinPurchaseSpacing,
    /// Presence marks an organizer the admin has approved to create events
    /// (issue #135).
    ApprovedOrganizer(Address),
    /// Total outstanding escrow balance across all events (issue #317).
    /// Used to prevent payment token changes while funds are in escrow.
    OutstandingEscrow,
    /// Event id at a position in an organizer's event list, keyed by the
    /// organizer and a zero-based index (issue #112). Lets the event list be
    /// read in pages without loading every event.
    OrganizerEventAt(Address, u32),
    /// Optional metadata URI for a ticket, set at issuance (issue #109).
    /// Stored apart from `Ticket` so tickets without one pay no extra cost.
    TicketMetadataUri(u64),
    /// Number of tickets an address owns, regardless of lifecycle status.
    /// Backs the SEP-50 `balance` read (issue #110).
    OwnerBalance(Address),
    /// Number of tickets in an owner's ticket index (issue #111). Tracked
    /// apart from `OwnerBalance` so index positions never depend on that
    /// counter's value.
    OwnerTicketCount(Address),
    /// Ticket id at a position in an owner's ticket index (issue #111).
    OwnerTicketAt(Address, u32),
    /// Position of a ticket inside its owner's index, so a transfer or burn can
    /// remove it in constant time (issue #111).
    TicketOwnerPos(u64),
    /// Number of times a ticket has been passed back out for re-entry (issue #78).
    /// Stored apart from `Ticket` so tickets that never re-enter pay no extra cost.
    ReentryCount(u64),
    /// Number of entries a ticket allows before it is fully used (issue #74).
    /// Absent means one use, so single-use tickets pay no extra cost.
    MaxUses(u64),
    /// Number of entries recorded against a ticket (issue #74).
    UsedCount(u64),
    /// Number of tickets issued for one of an event's tiers (issue #93).
    TierIssued(u64, String),
    /// Optional issuance cap for one of an event's tiers (issue #93). Absent
    /// means the tier is uncapped.
    TierCapacity(u64, String),
    /// Zones/gates a tier may enter, keyed by event id and tier (issue #80).
    /// Absent means the tier is unrestricted.
    TierZones(u64, String),
    /// Previous owners of a ticket, oldest first, capped at
    /// `MAX_OWNER_HISTORY` (issue #82). Absent until the first transfer.
    OwnerHistory(u64),
    /// Optional primary sale window `(sale_start, sale_end)` for an event, in
    /// ledger seconds (issue #94). Stored apart from `Event` so setting it
    /// never rewrites the event record and old records stay readable.
    SaleWindow(u64),
    /// Presence marks an address allowed to check tickets in for an event on
    /// the organizer's behalf, e.g. a gate device (issue #89).
    CheckInSigner(u64, Address),
    /// Contract paused status.
    Paused,
    /// Number of tickets purchased by a wallet for an event.
    WalletPurchaseCount(u64, Address),
    /// Allowlist member for an event.
    AllowlistMember(u64, Address),
    /// Platform fee taken from each primary sale, in basis points (issue #104).
    /// Absent means no fee.
    PlatformFeeBps,
    /// Funds the contract holds for resale bids and offers, across all tickets
    /// (issues #105, #106). Blocks a payment token change while non-zero.
    OutstandingResaleFunds,
    /// Timed auction for a ticket, keyed by ticket id (issue #105).
    Auction(u64),
    /// Escrowed offer from one buyer on one ticket (issue #106).
    Offer(u64, Address),
    /// Operator approved to move a ticket, keyed by ticket id (issue #107).
    /// Absent means no operator is approved.
    TicketApproval(u64),
    /// Perks attached to one of an event's tiers, e.g. "lounge access", keyed
    /// by event id and tier name (issue #68). Absent means the tier has no perks.
    TierPerks(u64, String),
    /// Bounded custom metadata map for an event (issue #70). Stored apart from
    /// `Event` so events without metadata pay no extra cost.
    EventMetadata(u64),
    /// Optional validity window `(valid_from, valid_until)` for a ticket, in
    /// ledger seconds (issue #72). Absent means the ticket is valid whenever
    /// the event allows check-in.
    TicketValidity(u64),
}

// Keep the persistent encoding beside the public and stored ticket types.
impl StoredTicket {
    pub(crate) fn pack(ticket: &Ticket) -> StoredTicket {
        StoredTicket {
            event_id: ticket.event_id,
            owner: ticket.owner.clone(),
            tier: ticket.tier.clone(),
            seat: ticket.seat.clone(),
            lifecycle: ((ticket.transfers as u64) << 8) | Self::status_code(&ticket.status),
            original_price: ticket.original_price,
            resale_price: ticket.resale_price,
            purchased_at: ticket.purchased_at,
        }
    }

    pub(crate) fn unpack(stored: StoredTicket) -> Result<Ticket, Error> {
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
            purchased_at: stored.purchased_at,
        })
    }

    fn status_code(status: &TicketStatus) -> u64 {
        match status {
            TicketStatus::Valid => 0,
            TicketStatus::Used => 1,
            TicketStatus::Revoked => 2,
            TicketStatus::Resale => 3,
            TicketStatus::Refunded => 4,
        }
    }

    fn status_from_code(code: u64) -> Result<TicketStatus, Error> {
        match code {
            0 => Ok(TicketStatus::Valid),
            1 => Ok(TicketStatus::Used),
            2 => Ok(TicketStatus::Revoked),
            3 => Ok(TicketStatus::Resale),
            4 => Ok(TicketStatus::Refunded),
            _ => Err(Error::InvalidTicketLifecycle),
        }
    }
}
