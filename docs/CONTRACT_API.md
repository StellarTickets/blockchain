# Contract API reference

Full signatures are in
[`contracts/ticketing/src/lib.rs`](../contracts/ticketing/src/lib.rs);
this is a quick-reference summary.

| Function | Caller | Effect |
|---|---|---|
| `initialize(admin, payment_token)` | admin | One-time setup |
| `create_event(organizer, event_id, name, category, max_resale_multiplier_bps, royalty_bps)` | organizer | Registers an event |
| `issue_ticket(organizer, event_id, to, tier, seat, price)` | organizer | Mints a ticket (off-chain payment already settled) |
| `purchase_primary(buyer, event_id, tier, seat, price)` | buyer | On-chain primary sale + mint |
| `transfer_ticket(from, ticket_id, to)` | owner | Direct transfer |
| `verify_ticket(ticket_id)` | anyone | Read-only lookup |
| `check_in(organizer, ticket_id)` | organizer | Marks used, one-way |
| `revoke_ticket(organizer, ticket_id)` | organizer | Permanently voids |
| `list_for_resale(owner, ticket_id, price)` | owner | Lists under the event's price cap |
| `cancel_resale(owner, ticket_id)` | owner | Pulls a listing |
| `buy_resale(buyer, ticket_id)` | buyer | Buys a listing, splits royalty |
| `get_event(event_id)` | anyone | Read-only event lookup |
| `get_ticket(ticket_id)` | anyone | Read-only ticket lookup |

## Error codes

See the `Error` enum in `lib.rs` for the full list; each variant maps
to a specific precondition failure (e.g. `AlreadyUsed`, `NotOwner`,
`ResalePriceExceedsCap`).
