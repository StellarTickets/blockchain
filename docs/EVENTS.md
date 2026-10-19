# On-chain events

The contract publishes Soroban events, defined with
`#[contractevent]` in `lib.rs`:

Failed calls do not publish failure events. Soroban rolls back the complete
failed transaction, including its events; see [`ERRORS.md`](ERRORS.md) for
the structured-error decision and recommended off-chain context.

## `TicketIssued`

Emitted by `issue_ticket` and `purchase_primary`.

| Field | Type | Notes |
|---|---|---|
| `ticket_id` | `u64` | topic |
| `event_id` | `u64` | |
| `owner` | `Address` | Ticket owner |
| `tier` | `String` | Ticket tier name |

## `TicketCheckedIn`

Emitted by `check_in`.

| Field | Type | Notes |
|---|---|---|
| `ticket_id` | `u64` | topic |
| `organizer` | `Address` | |

Indexers and the backend's reconciliation job can subscribe to these
instead of polling `get_ticket` for every ticket on every block.

## `TicketTransferred`

Emitted by `transfer_ticket` after the ownership change is saved (issue #115).

| Field | Type | Notes |
|---|---|---|
| `ticket_id` | `u64` | topic |
| `from` | `Address` | Owner before the transfer |
| `to` | `Address` | Owner after the transfer |

A rejected transfer publishes nothing. Batch transfers, gift claims and resale
purchases change ownership without publishing this event today, so indexers
that need every owner change must still read them from `owner_of`.

## Admin events

| Event | Emitted by | Fields |
|---|---|---|
| `ContractInitialized` | `initialize` | `admin` (topic), `payment_token` |
| `PurchaseThrottleUpdated` | `set_purchase_throttle` | `admin` (topic), `min_ledger_spacing` |
| `OrganizerApproved` | `approve_organizer` | `admin` (topic), `organizer` |
| `OrganizerRevoked` | `revoke_organizer` | `admin` (topic), `organizer` |
| `PaymentTokenProposed` | `propose_payment_token` | `admin` (topic), `new_token`, `apply_after_ledger` |
| `PaymentTokenChanged` | `apply_payment_token` | `admin` (topic), `old_token`, `new_token` |
| `ContractUpgraded` | `upgrade` | `admin` (topic), `new_wasm_hash` (see [UPGRADES.md](UPGRADES.md)) |
| `AdminProposed` | `propose_admin` | `admin` (topic), `new_admin` |
| `AdminTransferred` | `accept_admin` | `old_admin` (topic), `new_admin` |
