# Error reference

Every fallible entry point of the `ticketing` contract returns
`Result<T, Error>`. `Error` is a `#[contracterror]` enum declared in
[`contracts/ticketing/src/error.rs`](../contracts/ticketing/src/error.rs)
with an explicit `#[repr(u32)]` discriminant, so every variant has a
**stable numeric code** that integrators can branch on. The enum runs
from `1` to `34` with no gaps — code `0` is never produced by this
contract.

This page is the canonical error reference. Where another document shows
a shorter list, this one wins.

## Full error table

| Code | Variant | Meaning |
|---|---|---|
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that already has an admin |
| 2 | `NotInitialized` | Contract state was read before `initialize` ran (`Admin`, `PaymentToken` or `TokenDecimals` is absent from instance storage) |
| 3 | `EventNotFound` | No event is stored under `event_id` |
| 4 | `EventAlreadyExists` | `create_event` / `create_event_with_options` was called with an `event_id` that is already taken |
| 5 | `TicketNotFound` | No ticket is stored under `ticket_id` |
| 6 | `NotOrganizer` | The signing address is not the `organizer` of the ticket's event |
| 7 | `NotOwner` | The signing address is not the ticket's current `owner` |
| 8 | `AlreadyUsed` | The ticket has already been checked in; the action is not allowed after check-in |
| 9 | `Revoked` | The ticket was revoked by the organizer; revoked tickets are permanently dead |
| 10 | `NotForResale` | The ticket's status is not `Resale` (cancelling or buying a listing that does not exist) |
| 11 | `ResalePriceExceedsCap` | Listing price is above `original_price * max_resale_multiplier_bps / 10000` |
| 12 | `InvalidPrice` | `price` is negative on `issue_ticket` / `purchase_primary`, or `<= 0` on `list_for_resale` |
| 13 | `InvalidRoyalty` | `royalty_bps > 10000` (more than 100%) |
| 14 | `EscrowNotEnabled` | `release_escrow` was called for an event that never called `enable_escrow` |
| 15 | `EventNotEnded` | The current ledger sequence is still below `event.escrow_release_ledger` |
| 16 | `PurchaseTooSoon` | The buyer is inside the admin-configured primary-purchase throttle window |
| 17 | `NotAdmin` | The signing address is not the contract admin set at `initialize` |
| 18 | `EventAlreadyStarted` | `enable_escrow` was called after the event already issued at least one ticket |
| 19 | `InvalidEventTime` | `starts_at` is not strictly in the future (`starts_at <= ledger timestamp`) |
| 20 | `TransfersFrozen` | The event's transfer freeze window has started (`now >= starts_at - transfer_freeze_seconds`) |
| 21 | `ResaleClosed` | The event's resale cutoff has passed (`now >= starts_at - resale_cutoff_seconds`) |
| 22 | `InvalidLottery` | `price < 0`, `winner_count == 0`, `winner_count > entrants.len()`, or the entrant list contains a duplicate address |
| 23 | `GiftClaimNotFound` | No gift claim is stored for this `ticket_id` |
| 24 | `GiftClaimExpired` | The gift claim's `expires_at` is in the past |
| 25 | `InvalidSecret` | `sha256(secret)` does not equal the stored `secret_hash` |
| 26 | `InvalidExpiry` | `expires_at` is not in the future |
| 27 | `EmptyBatch` | A batch entry point was called with an empty `ticket_ids` vector |
| 28 | `BatchTooLarge` | A batch entry point was called with more than `MAX_BATCH_SIZE` (50) ids |
| 29 | `InvalidPaymentToken` | The supplied address failed the `decimals()` probe, so it is not a SEP-41 token contract |
| 30 | `NoPendingPaymentToken` | `apply_payment_token` was called with no change pending |
| 31 | `TimelockNotElapsed` | The current ledger sequence is still below the pending change's `apply_after_ledger` |
| 32 | `TicketsAlreadyIssued` | `set_event_payment_token` was called after the event issued at least one ticket |
| 33 | `TransferLimitExceeded` | The ticket has already reached the event's `max_transfers_per_ticket` |
| 34 | `ResalePriceBelowFloor` | Listing price is below `original_price * min_resale_multiplier_bps / 10000` (only possible when the event set a floor) |

### Grouping

| Group | Codes | Shared property |
|---|---|---|
| Initialization & ordering | 1, 2, 4, 18, 30, 31, 32 | Initialization, idempotency and sequencing constraints — rejected without touching ticket state |
| Lookup | 3, 5 | The referenced record does not exist in persistent storage |
| Authorization | 6, 7, 17 | The signer is authenticated but lacks the required role for the record |
| Ticket state machine | 8, 9, 10, 33 | The ticket's `status` or transfer counter forbids the action |
| Policy, pricing & timing | 11, 12, 13, 19, 20, 21, 22, 34 | Organizer-configured or time-based policy rejects the arguments |
| Gift claims | 23, 24, 25, 26 | The claim link is missing, expired, or the preimage does not match |
| Batching | 27, 28 | `Vec` arity is outside `1..=MAX_BATCH_SIZE` |
| Escrow, throttle & token config | 14, 15, 16, 29 | Escrow release, purchase throttle, and payment-token validation |

## Which entry points return which error

Two views of the same data. The second table lists the codes each of the
31 exported entry points can return; the first expands the codes that
appear on most of them, so you can see at a glance which entry points
are reachable from a "not found" or a role check.

| Code | Reachable from |
|---|---|
| 2 | every entry point that reads contract-wide configuration: `propose_payment_token`, `apply_payment_token`, `set_purchase_throttle`, `event_payment_token`, `purchase_primary`, `buy_resale`, `release_escrow`, `revoke_with_refund`, `token_decimals` |
| 3 | every entry point that loads an existing event: `event_payment_token`, `allocate_lottery`, `enable_escrow`, `set_event_payment_token`, `issue_ticket`, `purchase_primary`, `release_escrow`, `transfer_ticket`, `transfer_batch`, `claim_gift`, `check_in`, `check_in_batch`, `revoke_ticket`, `set_seat`, `revoke_with_refund`, `revoke_batch`, `list_for_resale`, `buy_resale` |
| 5 | every entry point that loads an existing ticket: `transfer_ticket`, `transfer_batch`, `create_gift_claim`, `claim_gift`, `verify_ticket`, `verify_tickets`, `check_in`, `check_in_batch`, `revoke_ticket`, `set_seat`, `revoke_with_refund`, `revoke_batch`, `list_for_resale`, `cancel_resale`, `buy_resale`, `get_ticket` |
| 6 | every organizer-authorized entry point |
| 7 | every owner-authorized entry point |
| 8 | `transfer_ticket`, `transfer_batch`, `create_gift_claim`, `claim_gift`, `check_in`, `check_in_batch`, `revoke_with_refund` |
| 9 | `transfer_ticket`, `transfer_batch`, `create_gift_claim`, `claim_gift`, `check_in`, `check_in_batch`, `set_seat` |

| Entry point | Error codes it can return |
|---|---|
| `initialize` | 1, 29 |
| `propose_payment_token` | 2, 17, 29 |
| `apply_payment_token` | 2, 17, 29, 30, 31 |
| `create_event` | 4, 13, 19 |
| `create_event_with_options` | 4, 13, 19 |
| `allocate_lottery` | 3, 6, 22 |
| `enable_escrow` | 3, 6, 18 |
| `set_event_payment_token` | 3, 6, 29, 32 |
| `event_payment_token` | 2, 3 |
| `issue_ticket` | 3, 6, 12 |
| `purchase_primary` | 2, 3, 12, 16 |
| `release_escrow` | 2, 3, 6, 14, 15 |
| `set_purchase_throttle` | 2, 17 |
| `transfer_ticket` | 3, 5, 7, 8, 9, 20, 33 |
| `transfer_batch` | 3, 5, 7, 8, 9, 20, 27, 28, 33 |
| `create_gift_claim` | 5, 7, 8, 9, 26 |
| `claim_gift` | 3, 5, 7, 8, 9, 20, 23, 24, 25, 33 |
| `verify_ticket` | 5 |
| `verify_tickets` | 5, 27, 28 |
| `check_in` | 3, 5, 6, 8, 9 |
| `check_in_batch` | 3, 5, 6, 8, 9, 27, 28 |
| `revoke_ticket` | 3, 5, 6 |
| `set_seat` | 3, 5, 6, 9 |
| `revoke_with_refund` | 2, 3, 5, 6, 8 |
| `revoke_batch` | 3, 5, 6, 27, 28 |
| `list_for_resale` | 3, 5, 7, 8, 9, 11, 12, 21, 34 |
| `cancel_resale` | 5, 7, 10 |
| `buy_resale` | 2, 3, 5, 10, 21, 33 |
| `get_event` | 3 |
| `get_ticket` | 5 |
| `token_decimals` | 2 |

In batch entry points (`transfer_batch`, `check_in_batch`, `revoke_batch`,
`verify_tickets`) the per-item errors are returned for the **first**
failing id and the whole transaction is rolled back — batch calls are
all-or-nothing, not partial.

## Retry semantics

A contract `Err` aborts the entire Soroban transaction, so no state
change is persisted when any of the codes above is returned. Every
contract error is therefore safe to surface and retry after fixing the
inputs; the distinction that matters for a backend is whether the
condition is **permanent** or **time/balance dependent**:

| Class | Codes | Retry strategy |
|---|---|---|
| Permanent — fix the request | 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 17, 19, 22, 23, 24, 25, 26, 27, 28, 29, 30, 32, 33, 34 | Do not retry automatically; return a 4xx-equivalent to the caller |
| Time-dependent — retry after waiting | 15, 16, 20, 21, 31 | Retry when the relevant timestamp or ledger sequence has passed |
| Transient operational | 1, 2, 14, 18 | Deployment or configuration error; retrying the same arguments will not help |

Codes 14 and 18 are listed as operational rather than permanent because
they are usually symptoms of a caller that skipped a setup step
(`enable_escrow` before `release_escrow`, or `enable_escrow` after
sales started) rather than of bad user input.

## Errors that are not in this enum

Not every failure surfaces as an `Error` code. A backend must also
handle:

- **Authorization failures.** A missing or wrong `require_auth()`
  signature is enforced by the host, not the contract, and is reported as
  an auth error rather than `NotOwner` / `NotOrganizer` / `NotAdmin`. The
  three contract codes above mean "the signature was valid but the role
  was wrong"; they are not what you get when nobody signed.
- **SEP-41 token contract errors.** `purchase_primary`, `buy_resale`,
  `revoke_with_refund` and `release_escrow` call the payment token
  contract. An insufficient balance, a frozen balance or a token-side
  limit surfaces as that token contract's own error (typically an
  `Error` from the token's enum), not one of the codes above. Always
  simulate first and decode the failure from the token contract when the
  invocation moves funds.
- **Storage archival.** Reading an event or ticket whose persistent entry
  has passed its TTL fails at the host level before the contract runs
  (see [`STORAGE_TTL.md`](STORAGE_TTL.md)). This is distinguishable from
  `EventNotFound` / `TicketNotFound`: an archived entry can be restored,
  a missing one never existed.
- **Arithmetic and resource failures.** Overflow checks are enabled in
  release builds (`overflow-checks = true` in the workspace
  `Cargo.toml`), so an out-of-range amount panics the contract rather
  than returning a code. Resource-limit failures surface as
  transaction-level errors from the network, not contract errors.

## Decoding an error code

`Error` is a `#[contracterror]` enum, so the code is the variant's
`u32` discriminant. Generated bindings give you the variant name rather
than a bare integer, which is what you want in application code. This
repository's TypeScript example drives the contract through the SDK's
`Contract` class against a contract id (see
[`scripts/examples/issue-and-verify.ts`](../scripts/examples/issue-and-verify.ts)),
which has no enum knowledge, so failures arrive as raw error values:

```ts
import { Contract } from "@stellar/stellar-sdk";

const contract = new Contract(contractId);
await contract.purchase_primary({ buyer, event_id, tier, seat, price });
// rejects with the raw contract error value on any code in this table
```

If you are reading raw results — a simulation, or a transaction you did
not submit through a binding — the code lives in an `ScVal::Error`
carrying `ScErrorType::Contract`; the same integer appears as
`contractCode` in the `scError` of a failed transaction result. Do
**not** assume a bare number comes back:

```ts
import { scValToNative } from "@stellar/stellar-sdk";

// `retval` is the return value of a failed simulation's result entry.
function contractErrorCode(retval) {
  if (!retval) return undefined;                 // host-level failure: no code
  let decoded;
  try {
    decoded = scValToNative(retval);
  } catch {
    return undefined;                           // not convertible: host/auth/token error
  }
  if (typeof decoded === "number") return decoded;
  // Depending on SDK version the error decodes to a wrapper rather than a
  // number, so accept the documented shapes instead of coercing blindly.
  return decoded?.contractCode ?? decoded?.code;
}
```

`Number(scValToNative(retval))` is the tempting one-liner and it is
wrong: coercing a wrapper object yields `NaN`, which then compares false
against every code in this document and looks like a decoder bug.

In Rust, generated bindings give you the `Error` enum directly, so
match on variants rather than on integers. Prefer matching the
*variant* over hardcoding integers in application code; the numeric
codes below are the wire contract and are what appears in logs.

## Stability

Treat the numeric codes as a stable wire contract:

- Codes are **append-only**. A new error gets the next free number;
  existing codes never change meaning.
- Renaming a variant is a source-level break for generated bindings but
  does not change its code.
- A transaction that hits a host-level failure (auth, token, archival)
  never returns a contract code, so "no code" is a real and common
  outcome — do not treat a missing code as a bug in the decoder.

## More documentation

- [`CONTRACT_API.md`](CONTRACT_API.md) — function-by-function reference
- [`STORAGE_TTL.md`](STORAGE_TTL.md) — storage lifetime policy
- [`INTEGRATION.md`](INTEGRATION.md) — backend integration guide
- [`STELLAR_CLI_EXAMPLES.md`](STELLAR_CLI_EXAMPLES.md) — CLI invocations per function
- [`contracts/ticketing/src/error.rs`](../contracts/ticketing/src/error.rs) — the enum itself
