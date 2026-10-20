# SEP-50 compliance review

[SEP-50](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0050.md)
defines a non-fungible token interface for Soroban. This page records which
parts of it the `ticketing` contract implements, which it does not, and why.
Read it alongside [CONTRACT_API.md](CONTRACT_API.md).

Tickets are not a full SEP-50 collection. The contract is a multi-event
ticketing system, and a ticket is a record in that system rather than a token
that can be moved between arbitrary contracts. The read surface is implemented
so wallets and explorers can answer "who holds this ticket", "how many does
this address hold" and "where is its metadata". The write surface is partial:
a single per-ticket operator approval and `transfer_from` exist, but
`approve_for_all` and the standard approval events do not.

## Token ID and balance types

- **Token ID:** `u64`. SEP-50 specifies an unsigned integer for `TokenID`, so
  a ticket id is a valid `TokenID`. Ids are global across events, not
  per-collection.
- **Balance:** `u64`, matching the token ID width as the standard suggests.

## Function support

| SEP-50 function | Status | Contract function | Notes |
| --- | --- | --- | --- |
| `balance(owner) -> Balance` | Implemented | `balance(owner)` | Counts every ticket the address owns, in any lifecycle status. Used and revoked tickets still count. |
| `owner_of(token_id) -> Address` | Implemented | `owner_of(token_id)` | Errors with `TicketNotFound` for an unknown id. |
| `token_uri(token_id) -> String` | Implemented | `token_uri(token_id)` | Returns the URI set by `issue_ticket_with_metadata`. Errors with `MetadataUriNotSet` instead of returning an empty string when no URI was set. |
| `transfer(from, to, token_id)` | Not implemented under the SEP name | `transfer_ticket(from, ticket_id, to)` | Same effect, different name and argument order. Clears resale and gift state, and is subject to transfer freeze, transfer limits and soulbound events. |
| `transfer_from(spender, from, to, token_id)` | Implemented | `transfer_from(spender, from, to, ticket_id)` | Same argument order as SEP-50. Needs the per-ticket approval below, and applies the same rules as `transfer_ticket` (transfer freeze, transfer limits, soulbound events, auctions). Publishes `TicketTransferred` (issue #107). |
| `approve(approver, approved, token_id, live_until_ledger)` | Partly implemented | `approve(owner, ticket_id, operator)` | No `live_until_ledger`. An approval holds until the ticket changes owner or `revoke_approval` is called. A ticket has one operator at a time, and a new approval replaces the old one. |
| `get_approved(token_id) -> Option<Address>` | Implemented | `get_approved(ticket_id)` | Returns `None` when no operator is approved. |
| `approve_for_all`, `is_approved_for_all` | Not implemented | — | Approvals are per ticket only. |
| `revoke_approval(owner, ticket_id)` | Extension, not in SEP-50 | `revoke_approval(owner, ticket_id)` | Clears the operator for one ticket. Idempotent. |
| `name()`, `symbol()` | Not implemented | — | The contract hosts many events, so there is no single collection name or symbol. Open question for maintainers. |

## Events

| SEP-50 event | Status |
| --- | --- |
| `transfer(from, to) -> token_id` | Partly emitted. `transfer_ticket` and `transfer_from` publish `TicketTransferred` with `from` and `to` (issue #115). Batch transfers, gift claims, resale purchases, accepted offers and settled auctions do not publish it yet. |
| `approve`, `approve_for_all` | Not emitted. Approvals publish no event. |
| `mint(to) -> token_id` | Not emitted under this name. `TicketIssued` carries the same information, with a different shape. |

**Gap:** SEP-50 indexers detect transfers from `transfer` events. Only direct
transfers through `transfer_ticket` publish one (`TicketTransferred`), so batch
transfers, gift claims and resale purchases are still invisible to those
indexers. The read functions above still give the current owner.

## Migration and storage notes

- `balance` is backed by a counter that is written on every mint and ownership
  change. A deployment that predates this change has no counter for tickets it
  already holds, so `balance` under-reports for those holders. This contract has
  no upgrade path (see [UPGRADES.md](UPGRADES.md)), so these features ship on a
  fresh deployment.
- The counter and metadata entries follow the TTL policy in
  [STORAGE_TTL.md](STORAGE_TTL.md). `extend_ticket_ttl` renews the owner's
  counter along with the ticket. A holder whose tickets are never renewed or
  moved can still have the counter expire, which reads as `0`.

## Recommended follow-up

1. Emit a SEP-50 `transfer` event from every path that changes an owner.
2. Decide whether `name` and `symbol` belong on a per-event basis or are
   omitted.
3. Decide whether to add `live_until_ledger` expiry and `approve_for_all` to
   the approval model, or keep it to one operator per ticket.
