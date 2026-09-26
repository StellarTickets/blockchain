# Glossary

- **Event** — an on-chain record of a concert, flight, sports match,
  etc. Created via `create_event`; owns a resale policy.
- **Ticket** — an on-chain asset tied to one `Event`, owned by exactly
  one `Address` at a time.
- **Tier** — a free-text label on a ticket (e.g. "GA", "VIP") set at
  issuance; not separately validated against the event.
- **Check-in** — the one-way `Valid -> Used` transition performed at
  the point of entry.
- **Resale cap** — `max_resale_multiplier_bps` on the event; the
  ceiling a ticket can be relisted for, relative to its original price.
- **Royalty** — `royalty_bps` on the event; the organizer's cut of
  every resale, paid atomically with the ownership transfer.

## Ticket lifecycle states

Every ticket transitions through a state machine with four possible states: `Valid`, `Used`, `Revoked`, and `Resale`. This section documents valid transitions and the semantics of each state.

### State definitions

**`Valid`** — The ticket has been issued or transferred and is ready to use. The owner may:
- Transfer it to another address (`transfer_ticket`)
- Check it in at entry (`check_in`, one-way to `Used`)
- List it for resale (`list_for_resale`, transition to `Resale`)
- Have it revoked by the organizer (`revoke_ticket`, one-way to `Revoked`)

**`Used`** — The ticket has been checked in and the attendee has entered the event. This is a terminal state for attendance — no further operations are possible on a used ticket.
- Cannot be transferred, resold, or revoked
- Prevents any state-changing operation (enforced by `AlreadyUsed` error)
- Read-only lookup via `verify_ticket` is still allowed

**`Revoked`** — The ticket has been permanently voided by the organizer (e.g., due to chargeback, policy violation, or refund). This is a terminal state.
- Cannot be transferred, resold, checked in, or transferred again
- Prevents any state-changing operation (enforced by `Revoked` error)
- Read-only lookup via `verify_ticket` is still allowed
- If a refund was issued via `revoke_with_refund`, the original price is returned to the ticket owner

**`Resale`** — The ticket is actively listed for resale under the event's resale cap. The owner may:
- Cancel the listing (`cancel_resale`, return to `Valid`)
- Keep the listing (no state change until buyer acts)
- Cannot check in or transfer while in this state

When a buyer purchases a resale listing via `buy_resale`:
1. The organizer's royalty is paid (in the payment token)
2. The remaining proceeds are paid to the original owner
3. Ownership is transferred to the buyer (state remains `Valid` for the new owner)

### State transition diagram

```
                    ┌─────────────────────────────────────────┐
                    │                                         │
              ┌─────▼─────┐                                   │
              │   Valid   │                                   │
              └─────┬─────┘                                   │
                    │                                         │
         ┌──────────┼──────────┬──────────────────┐           │
         │          │          │                  │           │
    transfer()  check_in()  revoke_ticket()  list_for_resale()│
         │          │          │                  │           │
         ▼          ▼          ▼                  ▼           │
   ┌─────────┐ ┌──────┐  ┌─────────┐      ┌───────────┐     │
   │ Valid   │ │ Used │  │ Revoked │      │  Resale   │     │
   │(new     │ │      │  │         │      │           │     │
   │ owner)  │ │(end) │  │ (end)   │      └─────┬─────┘     │
   └─────────┘ └──────┘  └─────────┘            │           │
                                           cancel_resale()   │
                                                 │           │
                                                 └───────────┘
```

### Example flows

**Primary sale and check-in:**
1. Organizer calls `issue_ticket(to=Alice, ...)` → Ticket enters `Valid` state owned by Alice
2. Alice checks in at the gate → Organizer calls `check_in(...)` → Ticket enters `Used` state
3. Alice is now admitted; ticket is marked used and cannot be transferred or resold

**Transfer and resale:**
1. Organizer calls `issue_ticket(to=Alice, price=100)` → Ticket enters `Valid` state
2. Alice transfers to Bob → Ticket remains `Valid`, now owned by Bob
3. Bob lists for resale at 120 → Ticket enters `Resale` state
4. Carol buys the resale → Organizer gets royalty (5% = 5), Bob gets 115, Carol owns ticket in `Valid` state
5. Carol checks in → Ticket enters `Used` state

**Revocation with refund:**
1. Organizer issues ticket to Alice for $100
2. Chargeback occurs → Organizer calls `revoke_with_refund(ticket_id, refund=100)` → Ticket enters `Revoked` state, Alice receives $100 refund
3. Ticket can never be used, transferred, or resold again

**Invalid transitions (all fail with specific errors):**
- `transfer_ticket` on a `Used` ticket → `AlreadyUsed`
- `check_in` on a `Revoked` ticket → `Revoked`
- `buy_resale` on a ticket not in `Resale` state → `NotForResale`
- `revoke_ticket` on a `Used` ticket → succeeds, but ticket is now `Revoked`

### Transitions table

| From State | Operation | To State | Error if fails |
|---|---|---|---|
| `Valid` | `transfer_ticket(...)` | `Valid` (new owner) | `NotOwner` |
| `Valid` | `check_in(...)` | `Used` | `NotOrganizer` |
| `Valid` | `revoke_ticket(...)` | `Revoked` | `NotOrganizer` |
| `Valid` | `list_for_resale(...)` | `Resale` | `ResalePriceExceedsCap` |
| `Used` | *any state-changing op* | — | `AlreadyUsed` |
| `Revoked` | *any state-changing op* | — | `Revoked` |
| `Resale` | `cancel_resale(...)` | `Valid` | `NotOwner` |
| `Resale` | `buy_resale(...)` | `Valid` (new owner) | `NotOwner` (of payment token balance) |
| `Resale` | `transfer_ticket(...)` | — | (blocked; must cancel first) |

### Authorization & state ordering

Per issue #206, every state-changing entry point checks authorization (`require_auth()`) **before** validating state. This ordering ensures that an unauthenticated caller who invokes `revoke_ticket` on a non-existent ticket gets an auth error, not `TicketNotFound` — preventing information leakage about which tickets exist.

See [`contracts/ticketing/src/lib.rs`](../contracts/ticketing/src/lib.rs) and the `require_auth_runs_before_business_validation` test in `test.rs` for details.
