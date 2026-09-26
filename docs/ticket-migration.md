# Ticket data migration notes

The `Ticket` type now includes `transfers`, initialized to `0` for newly
issued tickets. Existing deployments that stored tickets using the previous
layout must migrate or re-issue those records before upgrading the contract;
Soroban does not automatically backfill newly added contract-type fields.

`Event` now supports optional `min_resale_multiplier_bps` and
`max_transfers_per_ticket` controls through `create_event_with_options`.
Existing events retain their previous behavior (`None` for both options), and
the original `create_event` entrypoint remains available for ABI compatibility.
