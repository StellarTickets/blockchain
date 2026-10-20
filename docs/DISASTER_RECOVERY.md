# Disaster recovery: migrating to a new contract id

The `ticketing` contract has no upgrade hook (see [`UPGRADES.md`](UPGRADES.md)).
If a deployment has to be replaced — a critical bug, a compromised admin key,
or state that cannot be recovered — the only path is to deploy a new contract,
which gets a **new contract id**, and move live state onto it. This document
is the runbook for that migration (issue #66). It expands the short checklist
in [`MAINNET.md`](MAINNET.md#rollback--recovery).

## When to use this runbook

| Situation | Response |
|---|---|
| Bug that can be worked around by policy (e.g. pause sales, stop listing) | `pause` and fix off-chain; **no migration** |
| Bug that corrupts or blocks ticket state, payments or check-in | Full migration (this runbook) |
| Admin key compromised | `pause` immediately if the key is still yours, then full migration with a new admin |
| Persistent entries archived (TTL lapsed) | Restore them (see [`STORAGE_TTL.md`](STORAGE_TTL.md)); **no migration** |

Migration is expensive and visible to every ticket holder. Prefer the cheaper
response whenever it fully contains the problem.

## What carries over and what does not

The new contract starts empty. Migration replays state through the public
API, so some fields are recreated and some are lost.

| State | Carried over | How |
|---|---|---|
| Admin, payment token | yes | `initialize(admin, payment_token)` on the new contract |
| Approved organizers | yes | `approve_organizer` per organizer |
| Event ids and parameters | yes | Organizers call `create_event` / `create_event_with_options` with the **same** `event_id` |
| Event policy (capacity, windows, expiry, re-entry, splits, payment token, soulbound, tier prices/capacity/zones/perks, metadata, sale window, allowlist, check-in signers, platform fee) | yes | Replay each setter from the old contract's reads |
| Ticket owner, tier, seat, price, metadata URI | yes | `issue_ticket_with_metadata` / `issue_tickets` |
| Ticket validity windows, max uses | yes | `set_ticket_validity`, `set_ticket_max_uses` after reissue |
| **Ticket ids** | **no** | New ids come from the new contract's counter. Keep an old→new id map |
| `Used` / `Revoked` / `Refunded` status | partial | Do not reissue revoked or refunded tickets. For used tickets, reissue and `check_in`, or skip if the event is over |
| `purchased_at`, `transfers`, owner history, re-entry and use counts | **no** | Reset on reissue. Refund windows and `max_transfers_per_ticket` restart |
| Resale listings, auctions, offers, gift claims, approvals | **no** | Settled or cancelled on the old contract before cut-over (see step 3) |
| Escrowed funds | **no** | Released or refunded from the old contract; tokens are never moved by the migration itself |

## Runbook

### 1. Freeze the old contract

```sh
stellar contract invoke --id $OLD_ID --network mainnet --source admin \
  -- pause --admin $ADMIN
```

`pause` rejects every state-changing entry point, so the snapshot taken in
step 2 stays consistent. Announce the freeze to organizers and holders
through your usual channels.

### 2. Snapshot the old state

Read everything you need **before** deploying anything new. Restore any
archived entries first, otherwise reads fail with a missing-entry error.

- **Events:** for each approved organizer, page through
  `list_organizer_events(organizer, start, 50)` and call `get_event`,
  `sale_window`, `tier_zones`, `tier_perks`, `get_tier_price` and
  `event_metadata` per event and tier.
- **Tickets:** `total_tickets()` gives the id range. Ticket ids are allocated
  sequentially, so read `get_ticket(id)` for each id and `token_uri`,
  `ticket_validity`, `ticket_max_uses` and `ticket_used_count` alongside.
  Missing ids are burned tickets and are skipped.
- **Cross-check:** replay `TicketIssued`, `TicketTransferred` and
  `TicketCheckedIn` from your indexer (see [`EVENTS.md`](EVENTS.md)) and
  confirm the final owner of each ticket matches `get_ticket`.

Store the snapshot as a signed, versioned file (e.g. JSON in the deployment
records repo) and record its hash in [`../DEPLOYMENTS.md`](../DEPLOYMENTS.md).

### 3. Settle money held by the old contract

The old contract may hold tokens for escrowed primary sales, auction bids and
offers. Settle these **on the old contract** — the migration never transfers
tokens itself. Because the contract is paused, temporarily `unpause` for this
step only, or perform it before step 1 if the bug allows.

- Auctions: `settle_auction` once ended; otherwise the seller cancels via
  `cancel_resale` if there are no bids.
- Offers: `cancel_offer` refunds the buyer.
- Escrow: `release_escrow` once `escrow_release_ledger` is reached, or
  `revoke_with_refund` / `refund_primary` where buyers are owed money.

Confirm `OutstandingEscrow` and `OutstandingResaleFunds` are zero by checking
the old contract's token balance, then `pause` again.

### 4. Deploy and initialize the new contract

```sh
scripts/deploy.sh <identity> mainnet      # prints NEW_ID
stellar contract invoke --id $NEW_ID --network mainnet --source admin \
  -- initialize --admin $ADMIN --payment_token $TOKEN
```

Use a fresh admin key if the old one may be compromised. Re-apply
`set_platform_fee_bps` and `set_purchase_throttle`, and `approve_organizer`
for every organizer in the snapshot.

### 5. Recreate events

Each organizer (or a backend holding their delegated signature) calls
`create_event_with_options` with the **same `event_id`** and parameters as the
snapshot, then replays the per-event setters. Event-level setters such as
`set_event_transferable` and `set_event_payment_token` only work before any
ticket is issued, so run them now.

### 6. Reissue tickets

Reissue live tickets with `issue_tickets` in batches of at most
`MAX_BATCH_SIZE` (35), or `issue_ticket_with_metadata` where a metadata URI
must be kept. For each ticket, record `old_id → new_id` from the returned id
and then reapply `set_ticket_max_uses` and `set_ticket_validity`.

Reissuing counts toward `max_tickets` and tier capacity exactly like the
original mints, so set capacities to the snapshot values, not lower.

### 7. Cut over

1. Update `TICKETING_CONTRACT_ID` in the backend, indexer and scanner apps.
2. Publish the old→new ticket id map so wallets can relink holdings.
3. Add a row to [`../DEPLOYMENTS.md`](../DEPLOYMENTS.md) noting the old id,
   new id, snapshot hash and reason.
4. Leave the old contract **paused permanently**. Do not delete it — its
   history is the audit trail for the migration.

### 8. Verify

- `total_tickets()` on the new contract equals the number of reissued tickets.
- For a sample of holders, `tickets_of(owner, 0, 50)` and `balance(owner)`
  match the snapshot.
- A test check-in at a gate succeeds against the new id.

## Rehearse before you need it

Run this runbook end to end on testnet against a copy of production-like data
at least once per release. A migration attempted for the first time during an
incident will miss steps.

## Related

- [`UPGRADES.md`](UPGRADES.md) — why the contract is immutable
- [`STORAGE_TTL.md`](STORAGE_TTL.md) — restoring archived entries
- [`STORAGE_LAYOUT.md`](STORAGE_LAYOUT.md) — every key that holds state
- [`MAINNET.md`](MAINNET.md) — deployment checklist
