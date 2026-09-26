# Threat model

Scope: the `ticketing` contract in this repository, as deployed, plus
the minimum backend surface needed to reason about who can reach it.
Written for engineers reviewing changes to the contract and for the
auditor referenced in [`AUDIT.md`](AUDIT.md).

This is a design-time analysis, not the output of a third-party audit.
No audit has been performed yet — see
[`SECURITY.md`](../SECURITY.md) and the readiness criteria in
[`MAINNET.md`](MAINNET.md#readiness-criteria).

## Assets

Ranked by what an attacker wants:

| # | Asset | Where it lives | Impact if compromised |
|---|---|---|---|
| 1 | Funds escrowed by the contract | SEP-41 token balance at the contract address, tracked by `Event.escrow_balance` | Direct loss of organizers' sale proceeds |
| 2 | Ticket ownership | `Ticket.owner` in persistent storage | Counterfeit entry, resale fraud, chargeback evasion |
| 3 | Ticket validity | `Ticket.status` | Forged admission, double entry |
| 4 | Resale settlement | `buy_resale` royalty/seller transfers | Theft of resale proceeds or of the organizer's royalty |
| 5 | Platform configuration | `Admin`, `PaymentToken`, `MinPurchaseSpacing`, `PendingPaymentToken` in instance storage | Denial of service, or redirecting settlement to an attacker-chosen token |
| 6 | Availability | TTL of every live entry | An event becomes unusable and requires a redeploy + migration |

## Trust boundaries

```text
                        Stellar network (consensus, ordering, finality)
                                      │
        ┌─────────────────────────────┴─────────────────────────────┐
        │                  ticketing contract (WASM)                │
        │   immutable, no upgrade, no pause, no admin override      │
        └───────┬──────────────────────────────────┬───────────────┘
                │ SEP-41 calls                    │ reads
                ▼                                  ▼
   ┌────────────────────────┐          ┌──────────────────────────┐
   │ payment token contract │          │  anyone (scanners, RPC,  │
   │ (Stellar Asset or SAC) │          │  indexers, competitors)  │
   └────────────────────────┘          └──────────────────────────┘
                ▲
                │ builds XDR, submits, reconciles — never signs
   ┌────────────┴───────────┐
   │ backend (trusted, not  │──▶ Postgres cache (derived data only)
   │  trusted with keys)    │
   └────────────────────────┘
```

Trust assumptions the design makes explicitly:

- **The Stellar network is trusted** for consensus, transaction
  ordering and finality. The contract's race-resistance properties are
  stated in those terms and no further.
- **The contract is trusted** to behave as written. It is immutable
  ([`UPGRADES.md`](UPGRADES.md)), so a bug cannot be patched in place.
- **The payment token contract is trusted** to honor SEP-41. A token
  with a malicious or buggy implementation is outside this
  contract's control; see [`AUDIT.md`](AUDIT.md) for the audit scope
  boundary.
- **The backend is trusted but not trusted with keys.** The contract
  never verifies that a caller is "the backend" — it only knows Stellar
  addresses and their signatures. Any party holding the organizer key
  *is* the organizer, and a compromised backend that holds a user key
  compromises that user.
- **Nobody else is trusted.** The contract assumes every other address
  is adversarial.

## Actors

| Actor | Capabilities |
|---|---|
| Ticket holder | Signs for their own address: transfer, gift, list, cancel, buy, verify |
| Resale buyer | Signs for their own address; can buy any listed ticket |
| Organizer | Full control over their own events: issue, set seat, check in, revoke (+ refund), escrow, event token |
| Contract admin | `set_purchase_throttle`, `propose_payment_token`, `apply_payment_token` (and the `initialize` key) |
| Arbitrary attacker | Can call any entry point with any arguments, and pay any fee |
| Bot/speculator | Can win transaction ordering and bid fees; can mint any number of tickets for any event at a price of their own choosing (see *Economics and abuse*) |

Note the asymmetry that drives most of the findings below: the
**organizer is trusted for their own events but has no authority over
anyone else's**, and the **admin has no authority over any ticket at
all** — it cannot revoke, mint, or transfer. That separation is the
main defense against a compromised admin key.

## Attack surfaces and mitigations

### Authorization

| Attack | Mitigation in place |
|---|---|
| Forge a transfer without the owner's signature | Every owner-authorized entry point calls `require_auth()` on the address it is about to act for, before any state is read or written |
| Replay a signature from another call | Soroban auth entries are bound to the invocation's arguments and the contract ID; a signature for one call does not authorize another |
| Organizer of event A acts on event B's tickets | Each organizer check compares against the *ticket's* event, and ticket ids are globally unique, so a cross-event mix-up is not constructible |
| Compromised admin mints or revokes tickets | The admin has no per-ticket powers at all; `NotAdmin` is the only thing it can trigger |
| Unauthorized caller learns ticket state from error codes | Entry points demand auth as their first statement, so an unauthenticated caller learns nothing about existence, ownership or status from the error returned |

The last row has one documented exception, described under
*Residual risks* below.

### Money movement

| Attack | Mitigation in place |
|---|---|
| Overpay or underpay and mint a ticket for a different price | `purchase_primary` transfers exactly `price` and mints in the same transaction; there is no path where a mint happens without the matching transfer |
| Resale cap bypass by splitting transfers | `max_resale_multiplier_bps` is checked on the *listing* price, and the listing is the only way to reach `buy_resale` |
| Rounding theft in royalty split | Royalty and seller amounts are computed with integer division from the same `resale_price`, and `seller_amount = resale_price - royalty`; the split can never sum to more than the price |
| Reentrancy through a malicious token contract | No reentrancy guard of its own, but a failed token transfer aborts the whole transaction: `malicious_token_reentrancy_fails_safely_and_preserves_state` asserts a reentrant token cannot advance `tickets_issued` by more than the one ticket being purchased, and `purchase_primary_leaves_no_partial_state_when_token_transfer_fails` asserts no ticket is minted and no event state changes when the transfer fails. The payment token is still an arbitrary callee — see *Residual risks* |
| Drain escrow early | `release_escrow` requires the current ledger sequence to have reached `escrow_release_ledger` and the caller to be the organizer; the balance is zeroed before the transfer |
| Refund drain via `revoke_with_refund` | Payer is the organizer, payee is the ticket owner, and the amount is the ticket's own `original_price` — a caller cannot direct funds elsewhere or set an amount |
| Steal via a malicious payment token | The token address is fixed at `initialize` and can only be changed through a propose/apply pair with a ~1 day timelock, and the candidate is probed for SEP-41 compliance first. Per-event overrides are locked once any ticket is issued (`TicketsAlreadyIssued`) |

### Ticket lifecycle

| Attack | Mitigation in place |
|---|---|
| Double check-in / re-entry | `check_in` moves `Valid`/`Resale` to `Used` and rejects `Used` (`AlreadyUsed`); the transition is one-way |
| Use a revoked ticket | Revoked is terminal: every transfer, resale, claim, check-in and seat change rejects `Revoked` |
| Sell a ticket the seller no longer owns | `buy_resale` reads the listing and the owner in the same invocation and only settles against the stored listing price; a transfer clears the listing (`resale_price = 0`, status back to `Valid`), so a stale listing cannot be bought |
| Guess a gift secret | Only the SHA-256 digest is stored; `claim_gift` requires the preimage. Secrets must be high-entropy — a weak secret is guessable by brute force, since the contract cannot check entropy |
| Claim a gift after it should be void | `expires_at` is enforced, and any transfer, check-in or revoke deletes the claim |
| Forge `event_id` collisions to hijack an event | `EventAlreadyExists` prevents overwriting; the backend must allocate ids without collision (a ULID cast to `u64` does this) |

### Economics and abuse

| Attack | Mitigation in place | Limit |
|---|---|---|
| Scalping above face value | `max_resale_multiplier_bps` caps listings at a configured multiple of `original_price` | Per event; a cap above 10,000 bps is the organizer's choice |
| Undercutting the organizer to below face value | Optional `min_resale_multiplier_bps` floor | Only set when the organizer opts in |
| Bot land-grab on a high-demand drop | Admin-configurable purchase throttle, off by default | Repeat purchases by one address only; does nothing about the initial race across many addresses — see [`HIGH_DEMAND_QUEUE.md`](HIGH_DEMAND_QUEUE.md) |
| Rug pull by an organizer | Tickets already issued remain valid and verifiable; the organizer can revoke but cannot transfer or redirect funds | Revocation is unilateral and irreversible; see *Residual risks* |
| Griefing a batch | `MAX_BATCH_SIZE` (50) bounds every batch call so a single transaction cannot be made unboundedly expensive | A batch is all-or-nothing, so one bad id blocks the rest |
| Mint tickets far below face value | None on-chain. `purchase_primary` takes `price` as a caller argument and only rejects a negative value; `Event` stores no list price, so the amount a buyer pays is whatever that buyer submits | The face price exists only in the backend. A buyer who calls the contract directly can mint a ticket for 1 stroop, and because `original_price` is set from that same argument, the resale cap scales off the attacker's own number |
| Mint unlimited supply for a free event | None on-chain. `Event` has no maximum-supply field and `purchase_primary` never checks one; `tickets_issued` is a counter, not a limit | The purchase throttle, when the admin enables it, limits one address; it does not cap an event's supply. Inventory is enforced by the backend only, so it does not survive a direct call |

### Availability

| Attack | Mitigation in place | Limit |
|---|---|---|
| Fill the ledger with spam against the contract | Standard Soroban resource fees apply to every call | Fee market, not contract policy |
| DoS an event by making it unusable | None in-contract | A lapsed `Event` entry, or an organizer who revokes everything, is only recoverable by redeploy + migration ([`MAINNET.md`](MAINNET.md#rollback--recovery)) |
| Drain an event's escrow by making sales and never releasing | Organizer-controlled; escrow is opt-in per event and only before any ticket is issued | — |

## Out of scope

- Stellar protocol and validator-level security.
- Bugs in third-party token contracts.
- Backend and frontend application security, key custody, database
  integrity and API authorization — tracked in the
  [backend](https://github.com/StellarTickets/backend) and
  [frontend](https://github.com/StellarTickets/frontend) repos.
- Economic viability of the fee model.
- Legal, privacy and regulatory compliance (GDPR/CCPA for on-chain
  personal data) — tracked in
  [`MAINNET.md`](MAINNET.md#7-legal--compliance).

## Residual risks

Accepted, documented, and not mitigated by the current design:

1. **No upgrade, no pause, no emergency stop.** A discovered bug means
   a new deployment and a data migration. Recovery time is bounded by
   the migration job, not by the contract. This is a deliberate
   trade-off ([`UPGRADES.md`](UPGRADES.md)).
2. **Unilateral organizer revocation.** An organizer can void any ticket
   in their event at any time, including after check-in-adjacent
   activity, and `revoke_with_refund` is optional. There is no on-chain
   dispute path; the design for one is in
   [`DISPUTES.md`](DISPUTES.md). A user whose ticket is revoked has no
   on-chain recourse.
3. **Compromised organizer key.** Whoever holds the key controls every
   ticket in that organizer's events: mint unlimited comps, revoke
   everything, take escrow. Multisig and hardware-backed keys are
   deployment-time mitigations, not contract ones.
4. **Compromised admin key.** The admin can change the payment token
   (after the timelock) and the purchase throttle. It cannot touch
   tickets or funds already escrowed under the old token — which is
   precisely why the timelock exists — but a token change still
   disrupts every event that relies on the contract-wide token.
5. **First-come-first-served primary sales.** Ordering is by ledger
   sequence and fee bid. The throttle limits repeat purchases by one
   address, not the initial race, and commit-reveal is unimplemented.
6. **Price and supply are off-chain only.** `Event` holds no list price
   and no maximum supply. `purchase_primary` trusts the `price` argument
   for both the transfer and the minted ticket's `original_price`, and
   never checks a cap. Anyone can therefore mint a ticket for any
   non-negative amount — including zero — and an attacker who underprices
   their own ticket also lowers the resale ceiling that
   `max_resale_multiplier_bps` derives from it. The purchase throttle is
   the only in-contract control, it is off by default, and it constrains
   one address rather than an event's inventory. Any assumption that the
   contract enforces the face value of a ticket is wrong.
7. **Storage archival.** A persistent entry that is not written for
   ~31 days is archived and becomes unreadable to the contract. `Event`
   entries are extended only at creation, so a long-presale event can
   lapse. Detailed in [`STORAGE_TTL.md`](STORAGE_TTL.md).
8. **Unauthenticated initialization probe.** The three admin entry
   points call `require_admin`, which reads the stored admin and
   compares it *before* demanding a signature — the role check has to
   happen first to know whose signature to demand. An unauthenticated
   caller can therefore distinguish "not initialized" from "not admin"
   on those three functions. No ticket, event or fund state is
   disclosed.
9. **Arbitrary external callee.** The contract calls the payment token
   contract, which is an independent contract. It trusts SEP-41
   semantics and holds no reentrancy guard of its own; the mitigations
   are the ordering of state writes relative to transfers and the fact
   that no authorization is granted to the token.
10. **Lottery randomness.** `allocate_lottery` uses Soroban's on-chain
    PRNG to shuffle the organizer's own entrant list. The organizer picks
    the entrants, so the draw is only as fair as that list — it is not a
    commitment-based draw and is not verifiable in advance.
11. **Gift secrets are only as strong as the entropy the owner picks.**
    The contract stores a SHA-256 digest and compares it, so it cannot
    reject a guessable secret.
12. **Unaudited.** No third-party audit has been performed. Treat the
    deployment as testnet-only until one is, per
    [`SECURITY.md`](../SECURITY.md).

## Reporting a vulnerability

Do not open a public issue. Follow the private reporting process in
[`SECURITY.md`](../SECURITY.md).

## Review checklist for contract changes

When proposing a change to the contract, re-check it against this model:

- [ ] Does the new entry point demand `require_auth()` **before** any
      storage read or business-rule check?
- [ ] Can the new entry point move funds? If so, is the state write
      ordered before the transfer, and is the amount derived from
      stored state rather than from an argument?
- [ ] Does it widen any role? (A new capability for `admin`,
      `organizer`, or a new role is a trust-boundary change and must
      be called out explicitly.)
- [ ] Does it add a persistent entry? Then it needs an `extend_ttl`
      policy, and [`STORAGE_TTL.md`](STORAGE_TTL.md) needs updating.
- [ ] Does it add an error code? Then it must be the next free number
      and [`ERRORS.md`](ERRORS.md) needs a row.
- [ ] Does it emit an event? If a consumer is expected to react to it,
      it must actually be published on every path that changes the state
      — see the `buy_resale` gap in
      [`INTEGRATION.md`](INTEGRATION.md#what-it-does-not-emit).
- [ ] Is there a new way to reach a state that requires a redeploy to
      fix? If so, the "no upgrade" assumption needs revisiting.

## More documentation

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — storage layout and auth model
- [`ERRORS.md`](ERRORS.md) — full error table
- [`STORAGE_TTL.md`](STORAGE_TTL.md) — storage lifetime policy
- [`INTEGRATION.md`](INTEGRATION.md) — backend integration guide
- [`AUDIT.md`](AUDIT.md) — audit scope and known limitations
- [`DISPUTES.md`](DISPUTES.md) — dispute/chargeback design (unimplemented)
- [`HIGH_DEMAND_QUEUE.md`](HIGH_DEMAND_QUEUE.md) — commit-reveal design (unimplemented)
- [`SECURITY.md`](../SECURITY.md) — private vulnerability reporting
