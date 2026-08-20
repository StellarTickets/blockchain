# Architecture

## Storage layout

The `ticketing` contract uses three storage categories:

- **Instance storage**: `Admin`, `PaymentToken`, `NextTicketId` — set once at
  `initialize` and read on nearly every call.
- **Persistent storage, keyed by `Event(u64)`**: one entry per event,
  holding the organizer address, resale policy, and issuance counter.
- **Persistent storage, keyed by `Ticket(u64)`**: one entry per ticket,
  holding owner, tier, seat, status, and pricing.

## Why persistent storage for events and tickets

Instance storage is cheap to read but expires with the contract
instance's own TTL and isn't a good fit for data that individual
ticket owners depend on staying alive independently of contract
upgrades. Persistent storage entries are extended on every write
(`extend_ttl`) so an active ticket never lapses.

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
