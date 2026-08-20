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
