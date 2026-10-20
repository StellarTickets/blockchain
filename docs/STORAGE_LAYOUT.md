# Storage layout

Every piece of state the `ticketing` contract keeps on-chain lives under a
`DataKey` variant defined in
[`contracts/ticketing/src/types.rs`](../contracts/ticketing/src/types.rs).
This page documents each key, its value type, whether it is instance or
persistent storage, and when its TTL is extended. It is kept in lockstep
with `lib.rs` — if a key appears here and not in the code (or vice versa),
this document is wrong.

For the TTL policy itself — what `LEDGER_BUMP` / `LEDGER_THRESHOLD` mean,
what happens when an entry lapses, and the operator keepalive job — see
[`STORAGE_TTL.md`](STORAGE_TTL.md).

## Instance storage

Instance keys share a single storage entry and the contract instance's
TTL. One `instance().extend_ttl()` call (made at the top of almost every
entry point via `extend_instance_ttl`) extends all of them together.

| Key | Value type | Set by | Notes |
|---|---|---|---|
| `Admin` | `Address` | `initialize` | Contract admin; also marks the contract as initialized |
| `PaymentToken` | `Address` | `initialize`, `apply_payment_token` | Contract-wide SEP-41 payment token |
| `TokenDecimals` | `u32` | `initialize`, `apply_payment_token` | Cached decimals of the active payment token (issue #233) |
| `PendingPaymentToken` | `PendingPaymentToken` | `propose_payment_token` | Timelocked token change; removed on apply |
| `MinPurchaseSpacing` | `u32` | `set_purchase_throttle` | Minimum ledger spacing between a buyer's primary purchases; 0 disables |
| `NextTicketId` | `u64` | `initialize`, `mint` | Monotonic ticket-id allocator |
| `OutstandingEscrow` | `i128` | `purchase_primary`, `release_escrow`, `revoke_with_refund`, `refund_primary` | Total escrowed primary-sale proceeds across all events (issue #317) |
| `PlatformFeeBps` | `u32` | `set_platform_fee_bps` | Platform fee in basis points (issue #104); absent means no fee |
| `OutstandingResaleFunds` | `i128` | `place_bid`, `settle_auction`, `make_offer`, `accept_offer`, `cancel_offer` | Total resale bids/offers held (issues #105, #106) |
| `Paused` | `bool` | `pause`, `unpause` | Contract-wide kill switch; absent means unpaused |

## Persistent storage

Persistent entries each carry their own TTL and are extended with
`extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP)` on the writes listed below.
An entry that is never touched again after its last extension survives
~31 days, then is archived (see [`STORAGE_TTL.md`](STORAGE_TTL.md)).

### Event records

| Key | Value type | Extended on | Removed on |
|---|---|---|---|
| `Event(id)` | `Event` | `create_event`, `create_event_with_options`, and every write via `save_event` | never (cancelled events stay for audit) |
| `TicketsIssued(id)` | `u64` | `create_event*`, every mint via `increment_tickets_issued` | never |
| `TierPrice(id, tier)` | `i128` | `set_tier_price` | never (0 marks a free tier) |
| `TierIssued(id, tier)` | `u64` | every mint that hits the tier | never |
| `TierCapacity(id, tier)` | `u64` | `set_tier_capacity` | removed when capacity is cleared |
| `TierZones(id, tier)` | `Vec<String>` | `set_tier_zones` | removed when the zone list is emptied |
| `SaleWindow(id)` | `(Option<u64>, Option<u64>)` | `set_event_sale_window` | removed when both bounds are cleared |
| `TierPerks(id, tier)` | `Vec<String>` | `set_tier_perks` | removed when the perk list is emptied |
| `EventMetadata(id)` | `Map<String, String>` | `set_event_metadata`, `remove_event_metadata` | removed when the last entry is removed |
| `AllowlistMember(id, addr)` | `bool` (presence) | `add_to_allowlist` | `remove_from_allowlist` |
| `WalletPurchaseCount(id, addr)` | `u32` | `purchase_primary`, `refund_primary` | never |
| `OrganizerEvents(org)` | `u32` | `create_event*` | never |
| `OrganizerEventAt(org, idx)` | `u64` | `create_event*` | never |

### Ticket records

| Key | Value type | Extended on | Removed on |
|---|---|---|---|
| `Ticket(id)` | `StoredTicket` (packed) | every write via `save_ticket` | `burn_used_ticket` |
| `TicketMetadataUri(id)` | `String` | `issue_ticket_with_metadata`, `extend_ticket_ttl` | `burn_used_ticket` |
| `ReentryCount(id)` | `u32` | `pass_out` | never |
| `MaxUses(id)` | `u32` | `set_ticket_max_uses` | `burn_used_ticket` |
| `UsedCount(id)` | `u32` | `consume_use` | `burn_used_ticket` |
| `TicketValidity(id)` | `(Option<u64>, Option<u64>)` | `set_ticket_validity` | removed when both bounds are cleared, `burn_used_ticket` |
| `OwnerHistory(id)` | `Vec<Address>` | every ownership change via `record_previous_owner` | never |
| `Auction(id)` | `Auction` | `start_auction`, `place_bid`, `extend_ticket_ttl` | `settle_auction`, `cancel_resale` |
| `Offer(id, buyer)` | `Offer` | `make_offer` | `accept_offer`, `cancel_offer` |
| `TicketApproval(id)` | `Address` | `approve`, `extend_ticket_ttl` | any owner change, burn, `revoke_approval` |
| `GiftClaim(id)` | `GiftClaim` | `create_gift_claim` | claim, transfer, check-in, revoke, list |

### Owner index (issue #111)

These three keys form a swap-remove index over one owner's tickets, so
`tickets_of` is O(page) and a transfer removes the ticket in constant time.
The naming convention for index keys is documented in
[`types.rs`](../contracts/ticketing/src/types.rs) and summarised below.

| Key | Value type | Extended on | Removed on |
|---|---|---|---|
| `OwnerBalance(addr)` | `u64` | every mint and ownership change | never (lapsing reads as 0) |
| `OwnerTicketCount(addr)` | `u32` | every mint and ownership change | never |
| `OwnerTicketAt(addr, idx)` | `u64` | every mint and ownership change | on the ticket's last index slot during removal |
| `TicketOwnerPos(id)` | `u32` | every mint and ownership change | on removal from the owner's index |

### Throttle and check-in

| Key | Value type | Extended on | Removed on |
|---|---|---|---|
| `LastPurchaseLedger(buyer)` | `u64` | `record_purchase` | never (expires with buyer inactivity) |
| `CheckInSigner(id, signer)` | `bool` (presence) | `add_checkin_signer` | `remove_checkin_signer` |
| `ApprovedOrganizer(addr)` | `bool` (presence) | `approve_organizer` | `revoke_organizer` |

## Index naming convention (issue #52)

Index-style keys follow three shapes so a reader can tell how a key is
scanned without looking up its call sites:

1. **Ordered list slots** — `<Subject>At(subject_id, index)` returns the id
   stored at a zero-based position, with a companion
   `<Subject>Count(subject_id)` or `<Subject>Events(subject_id)` giving the
   list length. Examples: `OwnerTicketAt` / `OwnerTicketCount`,
   `OrganizerEventAt` / `OrganizerEvents`.
2. **Reverse position lookup** — `<Subject>Pos(subject_id)` maps an element
   back to its slot in the owner's list so removal is constant time.
   Example: `TicketOwnerPos`.
3. **Composite per-entity keys** — `<Subject>(parent_id, child_id)` (or
   `<Subject>(event_id, label)` for tier- and policy-scoped data) stores one
   entry per (parent, child) pair without a list walk. Examples:
   `TierPrice(event_id, tier)`, `AllowlistMember(event_id, member)`,
   `Offer(ticket_id, buyer)`, `CheckInSigner(event_id, signer)`.

When adding a new index, pick the shape that matches how it is read —
paged iteration uses (1), O(1) removal from a list uses (2), and membership
or per-child configuration uses (3). Do not rename existing variants: the
XDR encoding of each variant name is part of the storage ABI.

## Storage diagram

```text
Instance storage (one entry, shared TTL)
├── Admin                          → Address
├── PaymentToken                   → Address
├── TokenDecimals                  → u32
├── PendingPaymentToken            → PendingPaymentToken
├── MinPurchaseSpacing             → u32
├── NextTicketId                   → u64
├── OutstandingEscrow              → i128
├── PlatformFeeBps                 → u32
├── OutstandingResaleFunds         → i128
└── Paused                         → bool

Persistent storage (per-entry TTL, extended on write)
├── Event(id)                      → Event
├── TicketsIssued(id)              → u64
├── TierPrice(id, tier)            → i128
├── TierIssued(id, tier)           → u64
├── TierCapacity(id, tier)         → u64
├── TierZones(id, tier)            → Vec<String>
├── SaleWindow(id)                 → (Option<u64>, Option<u64>)
├── TierPerks(id, tier)            → Vec<String>
├── EventMetadata(id)              → Map<String, String>
├── AllowlistMember(id, addr)      → bool
├── WalletPurchaseCount(id, addr)  → u32
├── OrganizerEvents(org)           → u32
├── OrganizerEventAt(org, idx)     → u64
├── Ticket(id)                     → StoredTicket
├── TicketMetadataUri(id)          → String
├── ReentryCount(id)               → u32
├── MaxUses(id)                    → u32
├── UsedCount(id)                  → u32
├── TicketValidity(id)             → (Option<u64>, Option<u64>)
├── OwnerHistory(id)               → Vec<Address>
├── Auction(id)                    → Auction
├── Offer(id, buyer)               → Offer
├── TicketApproval(id)             → Address
├── GiftClaim(id)                  → GiftClaim
├── OwnerBalance(addr)             → u64
├── OwnerTicketCount(addr)         → u32
├── OwnerTicketAt(addr, idx)       → u64
├── TicketOwnerPos(id)             → u32
├── LastPurchaseLedger(buyer)      → u64
├── CheckInSigner(id, signer)      → bool
└── ApprovedOrganizer(addr)        → bool
```

## More documentation

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — why events and tickets use persistent storage
- [`STORAGE_TTL.md`](STORAGE_TTL.md) — TTL policy, archival, and the keepalive job
- [`SEP50.md`](SEP50.md) — how `OwnerBalance` backs the SEP-50 `balance` read
- [`contracts/ticketing/src/types.rs`](../contracts/ticketing/src/types.rs) — the `DataKey` enum itself
