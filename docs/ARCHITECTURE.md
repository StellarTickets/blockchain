# Architecture

## Storage layout

`DataKey` in `contracts/ticketing/src/lib.rs` defines the contract's five
storage keys. The Rust value types and storage categories are:

| `DataKey` variant | Key payload | Storage | Stored value | TTL behavior in this contract |
| --- | --- | --- | --- | --- |
| `Admin` | None | Instance | `Address` | Set during `initialize`; shares the instance storage TTL. |
| `PaymentToken` | None | Instance | `Address` | Set during `initialize`; shares the instance storage TTL. |
| `NextTicketId` | None | Instance | `u64` | Initialized during `initialize` and updated for each minted ticket; shares the instance storage TTL. |
| `Event(u64)` | `event_id: u64` | Persistent | `Event` | Explicitly extended when created by `create_event`; later issuance and purchase writes do not explicitly extend it. |
| `Ticket(u64)` | `ticket_id: u64` | Persistent | `Ticket` | Set and explicitly extended by `save_ticket` whenever a ticket is created or updated. |

The contract defines `LEDGER_THRESHOLD = 500_000` and
`LEDGER_BUMP = 535_679` ledgers (about 31 days at five seconds per ledger).
The instance TTL is extended in `initialize` with those values. Each event
entry is extended in `create_event`; each ticket entry is extended in
`save_ticket`. These are the only explicit TTL extension calls in `lib.rs`.
The threshold is the point below which Soroban extends an entry, and the bump
is the minimum remaining TTL requested by that call. Reads do not explicitly
extend entry TTLs. See [Soroban's TTL guide](https://developers.stellar.org/docs/build/guides/conventions/extending-wasm-ttl)
for the threshold and extension semantics.

## Why persistent storage for events and tickets

Instance storage is cheap to read but expires with the contract
instance's own TTL and isn't a good fit for data that individual
ticket owners depend on staying alive independently of contract
upgrades. Persistent entries have independent TTLs. Ticket entries are
extended on every write through `save_ticket`; event entries are extended
when created, while later updates to their issuance counter do not make an
explicit TTL extension call. See the table above for the exact behavior.

## Authorization model

Every state-changing function calls `require_auth()` on the account
that must have approved the action:

- `create_event`, `issue_ticket`, `check_in`, `revoke_ticket` — the
  event's organizer.
- `purchase_primary`, `buy_resale` — the buyer.
- `transfer_ticket`, `list_for_resale`, `cancel_resale` — the current
  owner.

There is no admin override for any of these — the admin set at
`initialize` is reserved for future platform-level configuration, not
per-ticket authority.

## Payment settlement

All monetary transfers go through a single SEP-41 `payment_token`
configured at `initialize`. `purchase_primary` and `buy_resale` are
the only functions that move funds; both do so atomically with the
ownership change in the same transaction.
