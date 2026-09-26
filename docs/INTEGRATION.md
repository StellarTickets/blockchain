# Integration guide (backend)

Audience: engineers building the
[StellarTickets/backend](https://github.com/StellarTickets/backend)
service (or any other server-side integration) against the `ticketing`
contract. The frontend never calls this contract directly — it asks the
backend, and the user's wallet signs — so everything here is about
server-side behavior.

For a conceptual overview see [`ARCHITECTURE.md`](ARCHITECTURE.md); for
the argument-by-argument function list see
[`CONTRACT_API.md`](CONTRACT_API.md).

## The shape of the integration

```text
                 build XDR                    sign
backend  ─────────────────────▶  browser  ─────────▶  backend
  │        (with simulation)      wallet             │  (signed XDR)
  │                                                    │
  ├──── submit ──▶ Stellar RPC ──▶ ledger               │
  └──── read ────▶ Stellar RPC / indexer                │
```

Three consequences drive everything below:

1. **The backend is not the signer.** It assembles transactions and hands
   them to a client for signature. A backend that holds a user's secret
   key has broken the non-custodial model the platform advertises.
2. **Reads and writes take different paths.** Reads are simulations or
   RPC queries and return immediately. Writes are built, signed by a
   human, submitted, and only then reflected in state.
3. **The contract is the source of truth.** Postgres holds a cache for
   search and listing. On any disagreement, the contract wins — so every
   user-visible read that matters (ownership, status) should be
   confirmed against the chain rather than served from the cache.

## Configuration

| Setting | Meaning |
|---|---|
| `TICKETING_CONTRACT_ID` | The deployed contract ID (starts with `C`). Set per environment; see the [backend README](https://github.com/StellarTickets/backend#environment) |
| Network passphrase | Must match the network the contract is deployed on. Testnet and mainnet IDs are not interchangeable |
| RPC endpoint | Public testnet RPC is fine for development; mainnet needs a provider you pay for |
| Payment token | Per deployment, set at `initialize`. Read it at runtime with `event_payment_token(event_id)` rather than hardcoding it |

Confirm you are pointed at the right network before any write: fetch the
contract with `get_event` and check the returned `organizer` matches what
you expect. A mismatched network passphrase fails at signature
verification, not with a readable error.

## The contract surface

31 entry points. Grouped by what the backend has to do with them:

| Group | Entry points |
|---|---|
| Reads (no signature) | `get_event`, `get_ticket`, `verify_ticket`, `verify_tickets`, `event_payment_token`, `token_decimals` |
| Organizer writes | `create_event`, `create_event_with_options`, `allocate_lottery`, `enable_escrow`, `set_event_payment_token`, `issue_ticket`, `set_seat`, `check_in`, `check_in_batch`, `revoke_ticket`, `revoke_with_refund`, `revoke_batch`, `release_escrow` |
| Owner writes | `transfer_ticket`, `transfer_batch`, `create_gift_claim`, `claim_gift`, `list_for_resale`, `cancel_resale` |
| Buyer writes | `purchase_primary`, `buy_resale` |
| Admin writes | `initialize`, `propose_payment_token`, `apply_payment_token`, `set_purchase_throttle` |

Which address must sign is not a parameter of the function call — it is
derived from state. `create_event` requires the `organizer` argument to
sign; `transfer_ticket` requires `from`; `check_in` requires the ticket's
event `organizer`. The address you pass as an argument is the address
whose signature is demanded, so passing the wrong one fails with an auth
error before the function body runs.

## Id conventions

| Value | On-chain type | Where it comes from |
|---|---|---|
| `event_id` | `u64` | **Chosen by the backend**, typically a ULID cast to `u64`. The contract does not allocate it; collisions are your problem (`EventAlreadyExists`) |
| `ticket_id` | `u64` | Allocated by the contract from a monotonic counter starting at `0`. Returned by `issue_ticket`, `purchase_primary` and `allocate_lottery`. Do not predict it — read it from the return value or from `TicketIssued` |
| `buyer`, `organizer`, `from`, `to`, `admin` | `Address` | Stellar account (`G…`) or contract (`C…`) addresses |

Because ticket ids are a single global counter, they are unique across
all events, not per event. A `(event_id, ticket_id)` composite key is
unnecessary; `ticket_id` alone is the primary key everywhere.

Ticket ids are also not secret. Anyone can call `verify_ticket` with any
id. Treat a ticket id as a public handle, not a bearer token.

## Amounts

All monetary arguments and return fields are `i128` in the payment
token's **smallest unit** (stroops for XLM: 1 XLM = 10,000,000 stroops).
Never pass a display amount.

```ts
import { nativeToScVal, scValToNative } from "@stellar/stellar-sdk";

const amount = nativeToScVal(15_000_000n, { type: "i128" }); // 1.5 XLM
const raw = scValToNative(returnValue);                     // bigint
```

To render amounts, call `token_decimals()` — it returns the decimals of
the contract-wide payment token, cached at `initialize` and refreshed on
every successful token change. For a per-event token override, use
`event_payment_token(event_id)` and get its decimals from the token
contract itself; the cached `token_decimals` value only tracks the
contract-wide token. Prices are per ticket, with no platform fee on top.

> **The contract does not know your prices or your inventory.**
> `Event` stores no list price and no maximum supply, and
> `purchase_primary` takes `price` from the caller and only rejects a
> negative amount. Your database is the sole enforcement point for both,
> and anyone can call the contract directly and bypass it: a caller can
> mint a ticket for 1 stroop, or mint as many as they like for a free
> event. Because the minted ticket's `original_price` is copied from that
> same caller-supplied argument, an underpriced ticket also lowers the
> resale ceiling derived from it. Treat the primary-sale path as
> "unverified until you reconcile it" and see
> [THREAT_MODEL.md](THREAT_MODEL.md#residual-risks).

## Time and ledger conversions

The contract mixes two clocks. Getting this wrong is the most common
source of "why is my event frozen already" bugs.

| Field | Unit | Source |
|---|---|---|
| `starts_at` | Unix timestamp, **seconds** | Compared against `env.ledger().timestamp()`; must be strictly in the future at creation (`InvalidEventTime`) |
| `transfer_freeze_seconds` | Seconds before `starts_at` | Transfers are rejected once `now >= starts_at - transfer_freeze_seconds` (`TransfersFrozen`) |
| `resale_cutoff_seconds` | Seconds before `starts_at` | Listings and purchases are rejected once `now >= starts_at - resale_cutoff_seconds` (`ResaleClosed`) |
| `escrow_release_ledger` | Ledger **sequence number** | `release_escrow` requires `env.ledger().sequence() >= escrow_release_ledger` (`EventNotEnded`) |
| `min_ledger_spacing` | Ledger sequence delta | Primary-purchase throttle; `0` disables it (`PurchaseTooSoon`) |
| `apply_after_ledger` | Ledger sequence number | Returned by `PaymentTokenProposed`; the admin waits for this ledger before `apply_payment_token` |
| `GiftClaim.expires_at` | Unix timestamp, seconds | Compared against the ledger timestamp (`GiftClaimExpired`) |

Useful reference points at ~5s per ledger:

| Interval | Ledgers |
|---|---|
| `PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS` | 17,280 (~1 day) |
| Storage TTL per extension | 535,679 (~31 days) — see [`STORAGE_TTL.md`](STORAGE_TTL.md) |

Convert a target date to a ledger sequence for escrow by reading the
current sequence at submission time and adding the delta, then re-read
it immediately before submitting — ledger sequences move while you
prepare. A safer pattern is to release on a *date* basis: read
`starts_at` and only submit `release_escrow` once the ledger timestamp
has passed it.

## Call patterns

### Read path

Reads need no signature, so a simulation is enough and costs nothing but
an RPC round trip:

```ts
const sim = await server.simulateTransaction(tx);
if (!rpc.Api.isSimulationSuccess(sim)) throw new Error(sim.error);
const ticket = scValToNative(sim.result?.[0]?.retval) as Ticket;
```

`verify_ticket` is the primitive gate scanners and the backend's
ownership screens should use. `verify_tickets` takes up to 50 ids in
one call — use it for batch listing and group check-in views rather
than looping single reads.

### Write path

```text
build (with simulation) → hand to client for signature → submit → poll → confirm
```

Use `prepareTransaction` (or `assembleTransaction` on a simulation you
already ran) so resource fees and footprints are filled in from a real
simulation, and so the restore preamble is applied when an entry has
been archived. A hand-built transaction that skips simulation will fail
against an archived entry even when the entry is restorable — see
[`STORAGE_TTL.md`](STORAGE_TTL.md).

Do not treat `sendTransaction` returning `PENDING` as success. Poll
`getTransaction` until the status is `SUCCESS` before updating the
database, and read the return value out of the result meta — for
`issue_ticket` and `purchase_primary` that value is the new
`ticket_id`.

The pattern used in this repo's reference client is in
[`scripts/examples/issue-and-verify.ts`](../scripts/examples/issue-and-verify.ts).

### Batching

`MAX_BATCH_SIZE` is **50**. `transfer_batch`, `check_in_batch`,
`revoke_batch` and `verify_tickets` all reject an empty vector
(`EmptyBatch`) and more than 50 ids (`BatchTooLarge`), and they are
all-or-nothing: one bad id in a batch of 50 rolls the whole
transaction back. Chunk client batches at 50, and on failure re-submit
per id to isolate the offender.

## Error handling

Every contract error code is listed in [`ERRORS.md`](ERRORS.md), which
also covers the failures that are *not* contract codes — missing
signatures, SEP-41 token failures, and archived storage entries. The
practical rules for a server:

- **Simulate first, always.** Almost every contract error is visible in
  the simulation, so you can reject a bad request before asking a user
  to sign anything.
- **Decode the code, branch on the class.** The codes split into
  permanent (bad request), time-dependent (wait and retry) and
  operational (deployment or configuration problem). Retrying a
  permanent error just burns fees.
- **A missing code is normal.** Auth failures, token contract errors
  and storage archival all surface without an `Error` code. Have a
  fallback branch that reports the raw failure rather than assuming the
  decoder is broken.
- **Never auto-retry a write that moved money.** Any error aborts the
  transaction, so a failed `buy_resale` moved nothing — but a
  *successful* one that timed out on the client still settled. Reconcile
  against the chain before resubmitting.

## Keeping state in sync

### What the contract emits

| Event | Emitted by |
|---|---|
| `TicketIssued` | `issue_ticket`, `purchase_primary`, `allocate_lottery` |
| `TicketCheckedIn` | `check_in`, `check_in_batch` |
| `ContractInitialized` | `initialize` |
| `PurchaseThrottleUpdated` | `set_purchase_throttle` |
| `PaymentTokenProposed` | `propose_payment_token` |
| `PaymentTokenChanged` | `apply_payment_token` |

### What it does not emit

**Ownership changes emit nothing.** `transfer_ticket`, `transfer_batch`,
`claim_gift` and `buy_resale` all change `Ticket.owner` without
publishing an event, and neither do `revoke_ticket`,
`revoke_with_refund`, `revoke_batch`, `list_for_resale`,
`cancel_resale`, `set_seat`, `enable_escrow` or `release_escrow`.

Consequences for the backend:

- An event-driven indexer alone will drift from chain state on resale
  and transfer activity. Pair it with a periodic reconciliation pass
  that re-reads tickets known to be mutable.
- Prefer subscribing for the high-volume, unambiguous signals
  (`TicketIssued`, `TicketCheckedIn`) and reconciling the rest.
- When a client reports a successful transaction, trust the
  transaction result; do not wait for an event that will never arrive.

### The contract as the tiebreaker

The database holds `Ticket.status` as a cache. When a cached row and
`get_ticket` disagree, overwrite the cache and prefer the chain — that
is the whole point of putting ownership on-chain. The reconciliation job
should compare on `owner`, `status`, `seat`, `transfers`,
`original_price` and `resale_price`.

## Storage keeper

The contract extends the TTL of a `Ticket` on every write, but extends
an `Event` only when it is created. Long-lived events therefore need an
off-chain keepalive job, otherwise their record can lapse and every
subsequent call fails at the storage layer. Run an
`ExtendFootprintTTLOp` sweep for events with no on-chain activity, at
least every ~20 days, and alert on storage-layer failures. Full policy
and cadence rationale: [`STORAGE_TTL.md`](STORAGE_TTL.md).

## Pre-flight checklist

- [ ] `TICKETING_CONTRACT_ID` set per environment, and verified with a `get_event` read
- [ ] Network passphrase matches the deployment network
- [ ] Payment token resolved at runtime with `event_payment_token`, not hardcoded
- [ ] Amounts converted to smallest units (`i128`); `token_decimals()` used for display
- [ ] `event_id` allocated by the backend and persisted before the first ticket is issued
- [ ] `ticket_id` read from the transaction return value, never predicted
- [ ] Every write simulated before being handed to a client for signature
- [ ] Writes confirmed by polling to `SUCCESS`, not by `sendTransaction` returning `PENDING`
- [ ] Error handling covers contract codes *and* non-code failures
- [ ] Batches chunked at 50
- [ ] Event subscription paired with a periodic reconciliation pass
- [ ] TTL keeper scheduled and alerting on storage-layer errors

## More documentation

- [`CONTRACT_API.md`](CONTRACT_API.md) — function reference
- [`ERRORS.md`](ERRORS.md) — full error table and retry classes
- [`STORAGE_TTL.md`](STORAGE_TTL.md) — storage lifetime policy and keeper
- [`EVENTS.md`](EVENTS.md) — event schemas for indexers
- [`GAS_AND_FEES.md`](GAS_AND_FEES.md) — fee model
- [`STELLAR_CLI_EXAMPLES.md`](STELLAR_CLI_EXAMPLES.md) — CLI invocation examples
- [`DEPLOYMENT.md`](DEPLOYMENT.md) — deployment and multisig admin setup
- [Stellar: invoke a contract function using SDKs](https://developers.stellar.org/docs/build/guides/transactions/invoke-contract-tx-sdk)
- [Stellar: signing Soroban contract invocations](https://developers.stellar.org/docs/build/guides/transactions/signing-soroban-invocations)
