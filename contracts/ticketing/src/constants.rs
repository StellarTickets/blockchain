//! Contract limits and basis-point denominators.
//!
//! Centralizes the numeric limits used by the ticketing contract so they
//! have one source of truth instead of scattered magic numbers.

/// Ledgers a persistent or instance entry is extended to on each bump
/// (~31 days at 5s/ledger, matches other Soroban tooling defaults).
pub(crate) const LEDGER_BUMP: u32 = 535_679;

/// Remaining TTL, in ledgers, below which an entry is bumped.
pub(crate) const LEDGER_THRESHOLD: u32 = 500_000;

/// Maximum number of tickets accepted by the batch entry points.
///
/// 35 keeps a full `issue_tickets` batch inside the network's transaction
/// write limit (200): each mint performs five persistent writes — the ticket
/// record, the owner's balance counter, and the three owner-index entries —
/// so 35 × 5 = 175 writes plus a small per-call overhead stays under 200.
pub const MAX_BATCH_SIZE: u32 = 35;

/// Ledgers that must pass between proposing and applying a payment token
/// change (~1 day at 5s/ledger).
pub const PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS: u32 = 17_280;

/// Basis-point denominator: 10_000 bps == 100%.
pub const BPS_DENOMINATOR: u32 = 10_000;

/// Seconds after an event's `starts_at` before its used tickets may be burned
/// (issue #113). Thirty days.
pub const USED_TICKET_RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;

/// Maximum accepted length, in bytes, for the event `name` (Issue #125).
pub const MAX_NAME_LEN: u32 = 64;

/// Maximum accepted length, in bytes, for the event `category` (Issue #125).
pub const MAX_CATEGORY_LEN: u32 = 32;

/// Maximum accepted length, in bytes, for a ticket `tier` or `seat` (Issue #126).
pub const MAX_TICKET_LABEL_LEN: u32 = 32;

/// Maximum accepted length, in bytes, for a ticket metadata URI (Issue #109).
pub const MAX_METADATA_URI_LEN: u32 = 256;

/// Maximum number of event ids returned by one `list_organizer_events` page
/// (Issue #112). Larger requested pages are clamped to this bound.
pub const MAX_PAGE_SIZE: u32 = 50;

/// Most zones a tier can be restricted to (issue #80). Bounds the stored list.
pub const MAX_TIER_ZONES: u32 = 16;

/// Most perks a tier can carry (issue #68). Bounds the stored list.
pub const MAX_TIER_PERKS: u32 = 16;

/// Maximum accepted length, in bytes, for one tier perk label (issue #68).
pub const MAX_PERK_LEN: u32 = 64;

/// Most custom metadata entries an event can carry (issue #70). Bounds the
/// stored map so reading it stays a fixed cost.
pub const MAX_EVENT_METADATA_ENTRIES: u32 = 16;

/// Maximum accepted length, in bytes, for an event metadata key (issue #70).
pub const MAX_EVENT_METADATA_KEY_LEN: u32 = 32;

/// Maximum accepted length, in bytes, for an event metadata value (issue #70).
pub const MAX_EVENT_METADATA_VALUE_LEN: u32 = 256;

/// Number of previous owners kept per ticket (issue #82). Older owners are
/// dropped so a frequently traded ticket's history stays a fixed size.
pub const MAX_OWNER_HISTORY: u32 = 5;

/// Stable lowercase labels for the optional common category vocabulary.
pub const CATEGORY_CONCERT: &str = "concert";
pub const CATEGORY_FLIGHT: &str = "flight";
pub const CATEGORY_SPORTS: &str = "sports";
pub const CATEGORY_FESTIVAL: &str = "festival";
pub const CATEGORY_CONFERENCE: &str = "conference";
pub const CATEGORY_OTHER: &str = "other";
