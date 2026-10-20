# Contract API reference

Full signatures are in
[`contracts/ticketing/src/lib.rs`](../contracts/ticketing/src/lib.rs);
this is a quick-reference summary.

`get_ticket` and the legacy `verify_ticket` implementation return the same
`Ticket` record and error codes. `get_ticket` is the canonical Soroban naming
convention; `verify_ticket` is deprecated but remains available so existing
clients do not break. It may be removed only in a future breaking deployment.

| Function | Auth requirement | Effect |
| --- | --- | --- |
| `initialize(admin, payment_token)` | `admin` | One-time setup; selects the contract-wide payment token. |
| `propose_payment_token(admin, new_token)` | `admin` | Starts the timelocked contract-wide payment-token change. |
| `apply_payment_token(admin)` | `admin` | Applies the pending payment-token change after its timelock. |
| `approve_organizer(admin, organizer)` | `admin` | Adds an organizer to the event-creation allowlist. |
| `revoke_organizer(admin, organizer)` | `admin` | Removes an organizer from the event-creation allowlist. |
| `is_approved_organizer(organizer)` | anyone | Reports whether an address is approved to create events. |
| `create_event(organizer, event_id, params)` | `organizer` | Registers an event using the default transfer/resale controls. The organizer must be approved. |
| `create_event_with_options(organizer, event_id, params, min_resale_multiplier_bps, max_transfers_per_ticket)` | `organizer` | Registers an event with optional resale-floor and transfer-limit controls. The organizer must be approved. |
| `allocate_lottery(organizer, event_id, entrants, winner_count, tier, price)` | `organizer` | Randomly selects entrants and mints complimentary/reserved tickets. |
| `enable_escrow(organizer, event_id, escrow_release_ledger)` | `organizer` | Enables escrow for primary-sale proceeds before any ticket is issued. |
| `set_event_payment_token(organizer, event_id, token)` | `organizer` | Sets or clears the event payment-token override before tickets are issued. |
| `event_payment_token(event_id)` | anyone | Resolves the event override or contract-wide payment token. |
| `set_event_transferable(organizer, event_id, transferable)` | `organizer` | Makes an event soulbound (`false`) or transferable. Only before any ticket is issued (issue #108). |
| `set_event_capacity(organizer, event_id, max_tickets)` | `organizer` | Caps total tickets an event may issue, or clears the cap with `None`. Rejects zero or a cap below `tickets_issued` with `InvalidCapacity` (issue #92). |
| `set_event_check_in_window(organizer, event_id, check_in_opens_seconds, ends_at)` | `organizer` | Opens check-in `check_in_opens_seconds` before `starts_at` and closes it after `ends_at` (both optional). `ends_at` must be after `starts_at` (issue #95). |
| `set_event_ticket_expiry(organizer, event_id, expires_at)` | `organizer` | Expires the event's tickets at `expires_at`, or clears the expiry with `None`. The expiry must be in the future (issue #76). |
| `set_event_max_reentries(organizer, event_id, max_reentries)` | `organizer` | Sets how many times a checked-in ticket may re-enter. `0` disables re-entry (issue #78). |
| `issue_ticket(organizer, event_id, to, tier, seat, price)` | `organizer` | Mints a ticket for an off-chain/previously settled payment. |
| `issue_ticket_with_metadata(organizer, event_id, to, tier, seat, price, metadata_uri)` | `organizer` | Mints a ticket like `issue_ticket` and attaches an optional metadata URI of at most 256 bytes (issue #109). |
| `set_tier_price(organizer, event_id, tier, price)` | `organizer` | Sets the primary-sale price for an event tier. |
| `get_tier_price(event_id, tier)` | anyone | Reads the configured primary-sale price for an event tier. |
| `set_tier_perks(organizer, event_id, tier, perks)` | `organizer` | Attaches VIP perk labels (e.g. "lounge access") to a tier. At most `MAX_TIER_PERKS` (16) labels of 1–64 bytes; an empty list removes them. Informational only (issue #68). |
| `tier_perks(event_id, tier)` | anyone | Perks attached to a tier; empty when none (issue #68). |
| `set_event_metadata(organizer, event_id, key, value)` | `organizer` | Sets or overwrites one custom metadata entry. Keys are 1–32 bytes, values 1–256 bytes, and the map holds at most `MAX_EVENT_METADATA_ENTRIES` (16) keys (issue #70). |
| `remove_event_metadata(organizer, event_id, key)` | `organizer` | Removes one custom metadata entry; a missing key is a no-op (issue #70). |
| `event_metadata(event_id)` | anyone | The event's custom metadata as a `Map<String, String>`; empty when none (issue #70). |
| `purchase_primary(buyer, event_id, tier, seat)` | `buyer` | Pays the configured tier price and atomically mints a ticket. |
| `release_escrow(organizer, event_id)` | `organizer` | Releases an ended event's escrowed proceeds to its organizer. |
| `set_purchase_throttle(admin, min_ledger_spacing)` | `admin` | Sets the minimum ledger spacing between a buyer's primary purchases. |
| `transfer_ticket(from, ticket_id, to)` | `from` (owner) | Transfers one ticket directly, clears resale/gift state and publishes `TicketTransferred` (issue #115). |
| `transfer_batch(from, ticket_ids, to)` | `from` (owner) | Atomically transfers a bounded batch of tickets. |
| `create_gift_claim(owner, ticket_id, secret_hash, expires_at)` | `owner` | Creates an expiring, secret-backed claim link for a ticket. |
| `claim_gift(recipient, ticket_id, secret)` | `recipient` | Claims a gift ticket with its secret before expiry. |
| `verify_ticket(ticket_id)` | anyone | Deprecated compatibility alias for `get_ticket`. |
| `is_ticket_valid(ticket_id)` | anyone | Returns whether a ticket is in `Valid` status. |
| `is_valid(ticket_id, owner)` | anyone | Returns whether a ticket belongs to `owner` and is valid; lookup errors return `false`. |
| `verify_tickets(ticket_ids)` | anyone | Reads a bounded batch of ticket records for scanners. |
| `check_in(organizer, ticket_id)` | `organizer` | Marks one ticket used. |
| `check_in_batch(organizer, ticket_ids)` | `organizer` | Atomically marks a bounded batch of tickets used. |
| `revoke_ticket(organizer, ticket_id)` | `organizer` | Permanently revokes one ticket. |
| `burn_used_ticket(organizer, ticket_id)` | `organizer` | Deletes a checked-in ticket's record once `starts_at + USED_TICKET_RETENTION_SECONDS` (30 days) has passed (issue #113). |
| `pass_out(organizer, ticket_id)` | `organizer` | Returns a checked-in ticket to `Valid` so it can re-enter, up to the event's `max_reentries`. Returns the new re-entry count (issue #78). |
| `reentry_count(ticket_id)` | anyone | Number of times a ticket has been passed out for re-entry (issue #78). |
| `set_ticket_max_uses(organizer, ticket_id, max_uses)` | `organizer` | Sets how many entries a ticket allows before it is fully used. Defaults to 1. Zero, or a value below the recorded uses, is rejected (issue #74). |
| `ticket_max_uses(ticket_id)` | anyone | Number of entries a ticket allows (issue #74). |
| `ticket_used_count(ticket_id)` | anyone | Number of entries recorded against a ticket (issue #74). |
| `set_ticket_validity(organizer, ticket_id, valid_from, valid_until)` | `organizer` | Limits check-in for one ticket to `[valid_from, valid_until]` (both inclusive, both optional), e.g. the days of a multi-day pass. `None` for both removes the window (issue #72). |
| `ticket_validity(ticket_id)` | anyone | The ticket's `(valid_from, valid_until)` window; `(None, None)` when unrestricted (issue #72). |
| `set_seat(organizer, ticket_id, seat)` | `organizer` | Reassigns a ticket's seat before transfer/check-in restrictions apply. |
| `revoke_with_refund(organizer, ticket_id, refund)` | `organizer` | Revokes a ticket and optionally refunds its original price. |
| `revoke_batch(organizer, ticket_ids)` | `organizer` | Atomically revokes a bounded batch of tickets. |
| `list_for_resale(owner, ticket_id, price)` | `owner` | Lists an owned ticket for resale within the event's price limits. |
| `cancel_resale(owner, ticket_id)` | `owner` | Cancels an active resale listing before the resale cutoff. |
| `buy_resale(buyer, ticket_id)` | `buyer` | Purchases a resale listing and settles royalty plus seller proceeds. |
| `get_resale_price_cap(ticket_id)` | anyone | Exposes the maximum allowed resale price for a ticket. |
| `quote_resale(ticket_id)` | anyone | Returns a read-only quote `(price, royalty, seller_amount)` for resale settlement. Errors for an auctioned ticket. |
| `start_auction(owner, ticket_id, reserve_price, ends_at)` | `owner` | Starts a timed auction on an owned ticket. `ends_at` must be in the future and no later than the resale cutoff (issue #105). |
| `place_bid(bidder, ticket_id, amount)` | `bidder` | Escrows a bid that beats the reserve and the current high bid, within the resale cap. Refunds the bidder it displaces (issue #105). |
| `settle_auction(ticket_id)` | anyone | Settles an auction after `ends_at`. The high bidder gets the ticket and the bid is split between royalty and seller (issue #105). |
| `get_auction(ticket_id)` | anyone | Reads the open auction for a ticket. |
| `make_offer(buyer, ticket_id, amount)` | `buyer` | Escrows an offer on a listed ticket, within the resale price band (issue #106). |
| `accept_offer(owner, ticket_id, buyer)` | `owner` | Accepts an offer made to the current owner and settles it like `buy_resale` (issue #106). |
| `cancel_offer(caller, ticket_id, buyer)` | buyer or offer's owner | Refunds an open offer. The buyer withdraws it, or the owner it was made to rejects it (issue #106). |
| `get_offer(ticket_id, buyer)` | anyone | Reads an open offer. |
| `approve(owner, ticket_id, operator)` | `owner` | Approves one operator to move the ticket with `transfer_from` (issue #107). |
| `revoke_approval(owner, ticket_id)` | `owner` | Removes the ticket's operator approval. |
| `get_approved(ticket_id)` | anyone | Returns the approved operator, or `None`. |
| `transfer_from(spender, from, to, ticket_id)` | `spender` (approved operator) | Moves a ticket on the owner's behalf, following SEP-50 argument order (issue #107). |
| `set_platform_fee_bps(admin, fee_bps)` | `admin` | Sets the platform fee on primary sales, in basis points. `0` turns it off (issue #104). |
| `platform_fee_bps()` | anyone | Reads the platform fee in basis points. |
| `get_event(event_id)` | anyone | Read-only event lookup. |
| `get_ticket(ticket_id)` | anyone | Read-only ticket lookup. |
| `owner_of(token_id)` | anyone | SEP-50 read: the address that currently owns a ticket (issue #110). |
| `balance(owner)` | anyone | SEP-50 read: how many tickets an address owns, in any status (issue #110). The standard names it `balance`, not `balance_of`. |
| `token_uri(token_id)` | anyone | SEP-50 read: the metadata URI set at issuance. Errors with `MetadataUriNotSet` when there is none (issue #109). |
| `extend_ticket_ttl(ticket_id)` | anyone | Renews a ticket's persistent-storage TTL; does not change ticket state. |
| `get_organizer_events(organizer)` | anyone | Returns the number of events registered by an organizer. |
| `list_organizer_events(organizer, start, limit)` | anyone | Returns one page of an organizer's event ids; `limit` is clamped to 50 (issue #112). Events created before this index existed are skipped. |
| `tickets_of(owner, start, limit)` | anyone | Returns one page of the ticket ids an address holds, in any status (issue #111). `limit` is clamped to 50. Order is not chronological, since removals move the last entry into the freed slot. Use `balance(owner)` for the count; the issue's `balance_of` name is not added, per the SEP-50 decision above. |
| `total_tickets()` | anyone | Returns the number of tickets ever issued across all events (issue #114). Never decreases, including for revoked, used or burned tickets. |
| `token_decimals()` | anyone | Returns decimals cached for the active payment token. |

## Batch operation atomicity

The state-changing batch operations (`transfer_batch`, `check_in_batch`, and
`revoke_batch`) are atomic. If any item fails validation, the call returns an
error and no changes from that call are committed, including changes made for
items processed before the failure.

> Resale cutoff semantics: once `ledger_timestamp >= starts_at - resale_cutoff_seconds`, all resale actions reject with `Error::ResaleClosed`. That policy is enforced by the contract's `resale_closed` helper and applies to `list_for_resale`, `buy_resale`, and `cancel_resale`.

## Scanner validity checks

`is_ticket_valid(ticket_id) -> Result<bool, Error>` returns `true` only for
`TicketStatus::Valid`. Used, revoked, and resale-listed tickets return `false`;
missing tickets return `TicketNotFound`, and calls before initialization return
`NotInitialized`. Invalid stored lifecycle codes return `InvalidTicketLifecycle`.
It requires no signature and does not change ticket data or storage TTL.

The existing `is_valid(ticket_id, owner) -> bool` additionally checks ownership
and returns `false` on lookup errors. Its signature is retained for deployed
clients; the status-only query uses a distinct name to preserve that ABI.
Neither query checks a ticket in: admission must still call `check_in` to mark
it used and prevent a second admission.

## `tickets_issued` semantics

`Event.tickets_issued` (returned by `get_event`) is a **monotonic, lifetime issuance counter** for the event — **not** the active or circulating ticket supply:

- **Incremented on mint:** `issue_ticket` and `purchase_primary` each increment `tickets_issued` by `1`, and `allocate_lottery` increments it by `winner_count`.
- **Never decremented:** Revoking tickets (`revoke_ticket`, `revoke_with_refund`, `revoke_batch`) or checking them in (`check_in`, `check_in_batch`) transitions the ticket's `status` to `TicketStatus::Revoked` or `TicketStatus::Used`, respectively, without decrementing `tickets_issued`.
- **Why it is monotonic:** The contract uses `tickets_issued > 0` as a one-way guard to lock event settlement parameters (`enable_escrow`, `set_event_payment_token`) once any ticket has ever been issued, even if all issued tickets are later revoked.
- **Computing active supply:** Integrators that need the count of currently usable tickets (`TicketStatus::Valid` or `TicketStatus::Resale`) must not read `tickets_issued` as active supply; instead, index on-chain issuance/check-in events and revocation transitions or inspect ticket statuses via `get_ticket` / `verify_tickets`.

## Error codes

See [`ERRORS.md`](ERRORS.md) for the full table of every `Error` variant
(1–41), which entry points return each one, and whether it is worth
retrying. The enum itself lives in
[`contracts/ticketing/src/error.rs`](../contracts/ticketing/src/error.rs).
