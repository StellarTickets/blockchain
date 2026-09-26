# Storage TTL policy

Soroban storage is not permanent. Every ledger entry has a
`live_until_ledger`; when the current ledger passes it, the entry stops
being live. This page documents what the `ticketing` contract does about
that, what an operator has to do, and where the policy has gaps.

Background on Soroban storage types and archival semantics lives in the
[Stellar state archival
docs](https://developers.stellar.org/docs/learn/fundamentals/contract-development/storage/state-archival).

## The two constants

Both are defined at the top of
[`contracts/ticketing/src/lib.rs`](../contracts/ticketing/src/lib.rs):

```rust
const LEDGER_BUMP: u32 = 535_679;     // ~31 days at 5s/ledger
const LEDGER_THRESHOLD: u32 = 500_000; // ~29 days at 5s/ledger
```

Every `extend_ttl` call in the contract uses the same pair:
`extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP)`.

Soroban's two-argument form means: *if this entry's remaining TTL is
below `LEDGER_THRESHOLD`, push `live_until_ledger` out to
`current_ledger + LEDGER_BUMP`; otherwise do nothing.* It never shortens
an entry's life, so the call is safe to make on every write.

| Quantity | Ledgers | Wall clock at 5s/ledger |
|---|---|---|
| `LEDGER_BUMP` — TTL granted by an extension | 535,679 | ~31.0 days |
| `LEDGER_THRESHOLD` — refresh trigger | 500,000 | ~28.9 days |
| Headroom between them (`BUMP - THRESHOLD`) | 35,679 | ~2.1 days |

Two consequences follow from the headroom being small, and both matter
operationally:

1. **~31 days is the survival window.** Right after an extending write
   the entry has 535,679 ledgers of life left. If nothing ever touches it
   again, that is how long it stays live.
2. **Writes close together do not refresh.** The entry only falls back
   under the 500,000 threshold after 35,679 ledgers (~2.1 days) of
   inactivity, so a write inside that window finds `extend_ttl` a no-op.
   That is intentional — refreshing on every write would pay rent
   repeatedly for the same entry — but it means "touched recently" and
   "TTL was extended" are not the same thing when reading the contract.

`LEDGER_BUMP` is comfortably below the network's maximum entry TTL
(`max_entry_ttl` is 6,312,000 ledgers in the ledger configuration the
test suite runs against, visible in
`contracts/ticketing/test_snapshots/`), so neither limit is the binding
constraint here.

## What is extended, and when

| Storage | Key | Extended on | Not extended on |
|---|---|---|---|
| Instance | `Admin`, `PaymentToken`, `TokenDecimals`, `PendingPaymentToken`, `MinPurchaseSpacing`, `NextTicketId` | `initialize` only | `propose_payment_token`, `apply_payment_token`, `set_purchase_throttle`, every `mint` that bumps `NextTicketId` |
| Persistent | `Event(id)` | `create_event`, `create_event_with_options` | `allocate_lottery`, `enable_escrow`, `set_event_payment_token`, `issue_ticket`, `purchase_primary`, `release_escrow` |
| Persistent | `Ticket(id)` | every write, via `save_ticket` | — |
| Persistent | `GiftClaim(id)` | `create_gift_claim` | — (removed on claim, transfer, check-in or revoke, which is the intended disposal) |
| Persistent | `LastPurchaseLedger(buyer)` | `record_purchase` | never removed; expires with the buyer's inactivity |

Two structural notes:

- **Instance storage is a single entry.** All instance keys share the
  contract instance's TTL, and one `instance().extend_ttl()` call
  extends the contract instance and its code entry as well. That is why
  `initialize`'s single call covers every instance key — and why there
  is nothing to gain from calling it again per key.
- **`Ticket` is the well-covered entry.** Every ticket mutation, in
  every code path, goes through `save_ticket`, which extends. A ticket
  that is transferred, listed, bought, checked in or revoked is
  refreshed as a side effect of that operation.

## The gap: `Event` is extended only at creation

This is the most important operational fact in this document.

`Event(id)` is written in six places after creation — `allocate_lottery`,
`enable_escrow`, `set_event_payment_token`, `issue_ticket`,
`purchase_primary` and `release_escrow` — and **none of them call
`extend_ttl`**. The only extension happens in the two `create_event*`
functions. `tickets_issued`, `escrow_balance`, `escrow_enabled` and
`payment_token` therefore get persisted but the entry's TTL is left to
decay from the moment the event was created.

In practice that means an event record is on the clock from
`create_event`, not from its last sale. An event created more than
~31 days before its `starts_at` — a long-presale concert, a seasonal
pass, an event created early and activated later — can have its
`Event` entry go live-less while the event is still very much active.
Sales on such an event fail at the host level, because the archival
check happens before the contract body runs; the contract's own
`EventNotFound` is never reached.

**Mitigations available today**, in order of preference:

1. Create events close to the start of sales rather than months ahead.
2. Run a TTL keeper (below) that keeps the entry alive.
3. Treat the redeploy-and-migrate path in
   [`MAINNET.md`](MAINNET.md#rollback--recovery) as the recovery
   mechanism if an event lapses.

A code fix — calling `extend_ttl` on the event key in the same helper
that writes it — is a one-line change per write site, but it is a
contract change and this document only records the current policy.

## What happens when an entry lapses

`Event`, `Ticket` and `GiftClaim` are **persistent** entries. When their
TTL reaches zero they are *archived*, not deleted:

- The entry stays in the ledger but cannot be read by contract code.
- A transaction whose footprint contains an archived persistent entry
  but does not list it for restore fails during the apply stage, before
  the contract is entered. This is why a lapsed event produces a
  storage error rather than `EventNotFound`.
- Since Protocol 23, the Stellar RPC simulation populates a restore
  preamble automatically when it detects an archived entry, and the SDK
  can perform the restore and retry. Manual `RestoreFootprintOp`
  remains available for cases where the automatic path is too large or
  the operator wants to pay the restoration cost instead of the user's.
- A restored entry comes back with the network's **minimum** TTL for a
  newly created entry, not the ~31 days this policy grants. It is
  immediately eligible for a fresh `extend_ttl`, so plan a keeper run
  right after a restore rather than assuming the original window came
  back.

This contract uses no temporary storage, so nothing here is subject to
permanent deletion on TTL expiry.

## Operator runbook: keeping state alive

Because the contract only extends what it writes, keeping a dormant
event or a long-held ticket alive is an off-chain job. There are two
mechanisms:

| Mechanism | When to use | Notes |
|---|---|---|
| `ExtendFootprintTTLOp` | Preferred for a passive keepalive | Extends the entries named in the transaction's read-only footprint without calling contract code. Must be the only operation in the transaction, and its resource fees must be set from a simulation. |
| A normal write call | When a state change is wanted anyway | Any ticket-touching call (`set_seat`, `list_for_resale` + `cancel_resale`, `check_in`) extends the `Ticket` entry as a side effect. Does not extend the `Event` entry. |

A workable keeper cadence: run the extend job roughly every 20 days
for every event and ticket that has had no on-chain activity, which
leaves a ~11-day margin against the ~31-day survival window. There is no
reason to run it more often than that: any extension issued less than
~2.1 days after the previous one is a no-op and pays nothing useful,
and any extension at all resets the full window.

Sizing the job:

- Read the entries you intend to extend from the RPC ledger entries
  endpoint, put them in the **read-only** footprint, simulate to get
  `minResourceFee`, and submit.
- Remember the extended entries' TTL is capped by the network's maximum
  entry TTL, not by `LEDGER_BUMP`.

## Monitoring

Alert on the conditions described in
[`MAINNET.md`](MAINNET.md#4-monitoring--observability), specifically:

- Any invocation that fails with a storage/archival error rather than a
  contract `Error` code — that is the signature of a lapsed entry.
- Events and tickets whose last on-chain write is older than ~20 days,
  especially events whose `starts_at` is still in the future.
- The age of the contract instance and code entries, which no
  contract call extends after `initialize`.

## Cost

`extend_ttl` is not free — it is one of the more expensive operations
per unit of work — but it is only paid when the call is actually
effective. The policy above deliberately spends rent only on writes that
are more than ~2.1 days apart, which is why the threshold is set so
close to the bump: a lower threshold would refresh on every write in a
busy sale burst and pay rent repeatedly for the same entry.

## More documentation

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — why events and tickets are in persistent storage
- [`ERRORS.md`](ERRORS.md) — including how archival differs from `EventNotFound`
- [`GAS_AND_FEES.md`](GAS_AND_FEES.md) — the fee model this cost sits inside
- [`INTEGRATION.md`](INTEGRATION.md) — keeper and reconciliation jobs for a backend
- [Stellar: state archival](https://developers.stellar.org/docs/learn/fundamentals/contract-development/storage/state-archival)
- [Stellar: state archival guides](https://developers.stellar.org/docs/build/guides/archival)
