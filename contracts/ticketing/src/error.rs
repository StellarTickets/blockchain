//! Contract error codes for the ticketing contract.

use soroban_sdk::contracterror;

/// Error codes returned by the ticketing contract entry points.
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
}
