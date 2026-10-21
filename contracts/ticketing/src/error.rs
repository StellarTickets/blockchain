//! Contract error codes for the ticketing contract.

use soroban_sdk::contracterror;

/// Error codes returned by the ticketing contract entry points.
#[allow(missing_docs)]
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The contract has already been initialized.
    AlreadyInitialized = 1,
    /// The contract has not been initialized yet.
    NotInitialized = 2,
    /// No event exists with the given id.
    EventNotFound = 3,
    /// An event already exists with the given id.
    EventAlreadyExists = 4,
    /// No ticket exists with the given id.
    TicketNotFound = 5,
    /// The caller is not the event's organizer.
    NotOrganizer = 6,
    /// The caller is not the ticket's owner.
    NotOwner = 7,
    /// The ticket has already been checked in.
    AlreadyUsed = 8,
    /// The ticket has been revoked by the organizer.
    Revoked = 9,
    /// The ticket is not listed for resale.
    NotForResale = 10,
    /// The resale price exceeds the event's resale cap.
    ResalePriceExceedsCap = 11,
    /// The supplied price is negative or otherwise invalid.
    InvalidPrice = 12,
    /// The supplied royalty exceeds 100%.
    InvalidRoyalty = 13,
    /// Escrow is not enabled for the event.
    EscrowNotEnabled = 14,
    /// The event has not reached its escrow release ledger yet.
    EventNotEnded = 15,
    /// The buyer purchased too recently under the purchase throttle.
    PurchaseTooSoon = 16,
    /// The caller is not the contract admin.
    NotAdmin = 17,
    /// The operation requires an event that has not started yet.
    EventAlreadyStarted = 18,
    /// The event start time is not in the future.
    InvalidEventTime = 19,
    /// Transfers are frozen for the event.
    TransfersFrozen = 20,
    /// Resale listings are closed for the event.
    ResaleClosed = 21,
    /// The lottery entrant list or winner count is invalid.
    InvalidLottery = 22,
    /// No gift claim exists for the ticket.
    GiftClaimNotFound = 23,
    /// The gift claim has expired.
    GiftClaimExpired = 24,
    /// The supplied gift secret does not match the stored hash.
    InvalidSecret = 25,
    /// The gift claim expiry is not in the future.
    InvalidExpiry = 26,
    /// The batch is empty.
    EmptyBatch = 27,
    /// The batch exceeds the maximum batch size.
    BatchTooLarge = 28,
    /// The proposed token is not a valid token contract.
    InvalidPaymentToken = 29,
    /// There is no pending payment token change to apply.
    NoPendingPaymentToken = 30,
    /// The payment token change timelock has not elapsed.
    TimelockNotElapsed = 31,
    /// Tickets have already been issued for the event.
    TicketsAlreadyIssued = 32,
    /// The ticket has reached its event's ownership transfer limit.
    TransferLimitExceeded = 33,
    /// The resale price is below the event's configured resale floor.
    ResalePriceBelowFloor = 34,
    /// The event has no primary sale price configured for the requested tier.
    TierPriceNotSet = 35,
    /// The resale multiplier is below the 10_000 bps face-value floor.
    InvalidMultiplier = 36,
    /// The supplied name or category is empty.
    EmptyNameOrCategory = 37,
    /// The supplied name, category, tier or seat exceeds its maximum length.
    StringTooLong = 38,
    /// A ticket cannot be transferred to its current owner.
    SelfTransfer = 39,
    /// The seller cannot buy their own resale listing.
    SelfPurchase = 40,
    /// The caller is not on the admin-managed organizer allowlist.
    OrganizerNotApproved = 41,
    /// The ticket is listed for resale and cannot be checked in until cancelled.
    ResaleListingActive = 42,
    /// A stored ticket lifecycle word contains an unknown status code.
    InvalidTicketLifecycle = 43,
    /// The proposed payment token is the same as the currently active one.
    SamePaymentToken = 44,
    /// Arithmetic overflow in financial calculation (e.g. resale cap or royalty calculation).
    MathOverflow = 45,
    /// The event is soulbound: its tickets cannot be transferred, gifted,
    /// listed for resale or bought on the resale market (Issue #108).
    NotTransferable = 46,
    /// The ticket was issued without a metadata URI (Issue #109).
    MetadataUriNotSet = 47,
    /// The ticket has not been checked in, so it cannot be burned (issue #113).
    TicketNotUsed = 48,
    /// The used ticket's retention period has not elapsed yet (issue #113).
    RetentionPeriodNotElapsed = 49,
    /// The event has reached its maximum ticket count (issue #92).
    SoldOut = 50,
    /// The capacity is below the number of tickets already issued (issue #92).
    InvalidCapacity = 51,
    /// Check-in is outside the event's configured window (issue #95).
    CheckInWindowClosed = 52,
    /// The check-in window is invalid, e.g. it ends before the event starts (issue #95).
    InvalidCheckInWindow = 53,
    /// The ticket has passed its expiry time (issue #76).
    TicketExpired = 54,
    /// The ticket expiry is not in the future (issue #76).
    InvalidTicketExpiry = 55,
    /// The event does not allow re-entry, or the ticket is not checked in (issue #78).
    ReentryNotAllowed = 56,
    /// The ticket has used all of its allowed re-entries (issue #78).
    ReentryLimitReached = 57,
    /// The event has been cancelled by its organizer, so it can no longer
    /// issue, sell or list tickets, and cannot be cancelled again (issue #91).
    EventCancelled = 58,
    /// The tier has reached its issuance cap (issue #93).
    TierSoldOut = 59,
    /// The max uses is zero, or below the number of uses already recorded (issue #74).
    InvalidMaxUses = 60,
    /// A gift claim already exists for the ticket and has not yet been claimed or expired.
    GiftClaimAlreadyExists = 66,
    /// The contract is currently paused by the admin.
    ContractPaused = 67,
    /// The refund window for this ticket has expired.
    RefundWindowExpired = 68,
    /// The ticket has already been refunded.
    AlreadyRefunded = 69,
    /// The wallet has reached its purchase limit for this event.
    PurchaseLimitExceeded = 70,
    /// The caller is not on the event's allowlist during presale.
    NotOnAllowlist = 71,
    /// The presale window has not started yet.
    PresaleNotStarted = 72,
    /// The presale window has ended.
    PresaleEnded = 73,
    /// The platform fee is above 100% (10_000 bps) (issue #104).
    InvalidPlatformFee = 74,
    /// No auction exists for the ticket (issue #105).
    AuctionNotFound = 75,
    /// The auction has closed, so bids are no longer accepted (issue #105).
    AuctionEnded = 76,
    /// The auction is still open and cannot be settled yet (issue #105).
    AuctionNotEnded = 77,
    /// The ticket has an open auction, so this operation is blocked until it settles (issue #105).
    AuctionActive = 78,
    /// The auction already has a bid and can no longer be cancelled (issue #105).
    AuctionHasBids = 79,
    /// The auction end is not in the future, or falls after resale closes (issue #105).
    InvalidAuctionEnd = 80,
    /// The bid does not beat the highest bid, or does not meet the reserve (issue #105).
    BidTooLow = 81,
    /// The buyer has no open offer on the ticket (issue #106).
    OfferNotFound = 82,
    /// The buyer already has an open offer on the ticket (issue #106).
    OfferAlreadyExists = 83,
    /// The spender is not the operator approved for the ticket (issue #107).
    NotApproved = 84,
    /// The payment token cannot change while resale bids or offers are held (issues #105, #106).
    ResaleFundsOutstanding = 85,
    /// The ticket's tier may not enter the zone being checked in at (issue #80).
    ZoneNotAllowed = 86,
    /// Too many zones, or a zone label is empty or too long (issue #80).
    InvalidZones = 87,
    /// The primary sale has not opened yet (issue #94).
    SaleNotStarted = 88,
    /// The primary sale has closed (issue #94).
    SaleEnded = 89,
    /// The sale window ends at or before it starts (issue #94).
    InvalidSaleWindow = 90,
    /// The ticket's validity window has not opened yet (issue #72).
    TicketNotYetValid = 91,
    /// The ticket's validity window has closed (issue #72).
    TicketValidityEnded = 92,
    /// The validity window ends at or before it starts (issue #72).
    InvalidValidityWindow = 93,
    /// Too many perks, or a perk label is empty or too long (issue #68).
    InvalidPerks = 94,
    /// An event metadata key or value is empty or too long (issue #70).
    InvalidEventMetadata = 95,
    /// The event already holds `MAX_EVENT_METADATA_ENTRIES` metadata entries (issue #70).
    EventMetadataFull = 96,
}
