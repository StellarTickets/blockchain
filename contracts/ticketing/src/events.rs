//! Contract events emitted by the ticketing contract.
#![allow(missing_docs)]

use soroban_sdk::{contractevent, Address, BytesN, String};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when a ticket is minted.
pub struct TicketIssued {
    #[topic]
    pub ticket_id: u64,
    pub event_id: u64,
    pub owner: Address,
    pub tier: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted once when the contract is initialized.
pub struct ContractInitialized {
    #[topic]
    pub admin: Address,
    pub payment_token: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin changes the purchase throttle.
pub struct PurchaseThrottleUpdated {
    #[topic]
    pub admin: Address,
    pub min_ledger_spacing: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin proposes a payment token change.
pub struct PaymentTokenProposed {
    #[topic]
    pub admin: Address,
    pub new_token: Address,
    pub apply_after_ledger: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when a proposed payment token change is applied.
pub struct PaymentTokenChanged {
    #[topic]
    pub admin: Address,
    pub old_token: Address,
    pub new_token: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when a ticket is checked in at the point of entry.
pub struct TicketCheckedIn {
    #[topic]
    pub ticket_id: u64,
    pub organizer: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin adds an organizer to the event-creation allowlist.
pub struct OrganizerApproved {
    #[topic]
    pub admin: Address,
    pub organizer: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin removes an organizer from the event-creation allowlist.
pub struct OrganizerRevoked {
    #[topic]
    pub admin: Address,
    pub organizer: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the organizer cancels an event (issue #91).
pub struct EventCancelled {
    #[topic]
    pub event_id: u64,
    pub organizer: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when a ticket changes owner through `transfer_ticket` (issue #115).
pub struct TicketTransferred {
    #[topic]
    pub ticket_id: u64,
    pub from: Address,
    pub to: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin replaces the contract WASM with `upgrade` (issue #101).
pub struct ContractUpgraded {
    #[topic]
    pub admin: Address,
    pub new_wasm_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin nominates a new admin with `propose_admin` (issue #100).
pub struct AdminProposed {
    #[topic]
    pub admin: Address,
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the pending admin accepts with `accept_admin` (issue #100).
pub struct AdminTransferred {
    #[topic]
    pub old_admin: Address,
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
/// Emitted when the admin sets the platform fee on resales (issue #103).
pub struct PlatformFeeUpdated {
    #[topic]
    pub admin: Address,
    pub fee_bps: u32,
    pub recipient: Address,
}
