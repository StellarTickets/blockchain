# Changelog

All notable changes to the `ticketing` contract are documented here.
This project follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Fixed
- Resolve a duplicate error discriminant: `ZoneNotAllowed` moves from `61`
  to `66`, since `61` was already `GiftClaimAlreadyExists`. The crate could
  not compile with both.
- Repair merged organizer approval and packed ticket lookup definitions, and
  update a stale event-creation test so the contract builds again.

### Changed
- Centralize organizer authorization and terminal ticket status checks without
  changing operation-specific revocation rules (#348, #349).
- Keep ticket lifecycle encoding with the types in `types.rs`, preserving
  crate-root exports and byte-identical WASM for the isolated move (#347).
- Upgrade `soroban-sdk` from 26 to 28. See [CONTRIBUTING.md](CONTRIBUTING.md)
  for the dependency versioning policy and upgrade procedure.

### Added
- Contract version: `version()` returns the crate version from `Cargo.toml`
  (#102).
- Admin-only upgrade: `upgrade(admin, new_wasm_hash)` replaces the contract
  code and keeps storage. See [docs/UPGRADES.md](docs/UPGRADES.md) for the
  procedure and state migration rules (#101).
- Two-step admin transfer: `propose_admin` nominates a successor and
  `accept_admin` completes the handover, which needs the nominee's auth
  (#100).
- Platform fee on resales: `set_platform_fee(admin, fee_bps, recipient)` takes
  a share of each resale from the seller's proceeds, paid to `recipient`.
  Fee plus any event royalty may not exceed 10000 bps (`InvalidPlatformFee`).
  `quote_resale` now reports a `seller_amount` net of the fee. `platform_fee`
  reads the setting (#103).
- Multi-use tickets (passes): `set_ticket_max_uses` sets how many entries a
  ticket allows (default 1). `check_in` and `check_in_batch` count each entry,
  and a ticket becomes `Used` only once its uses are exhausted. `pass_out`
  returns the use its entry consumed. `ticket_max_uses` and
  `ticket_used_count` read the counters (#74).
- Event capacity limit: `set_event_capacity` caps the tickets an event may
  issue. `issue_ticket`, `purchase_primary` and `allocate_lottery` reject a
  mint that would exceed it with `SoldOut`; a cap below `tickets_issued` is
  rejected with `InvalidCapacity` (#92).
- Check-in window: `set_event_check_in_window` opens check-in
  `check_in_opens_seconds` before `starts_at` and closes it after `ends_at`.
  `check_in` and `check_in_batch` reject out-of-window check-in with
  `CheckInWindowClosed`; `ends_at` must follow `starts_at` (#95).
- Ticket expiry: `set_event_ticket_expiry` ends ticket validity at a timestamp.
  `check_in`, `check_in_batch` and every transfer path reject expired tickets
  with `TicketExpired` (#76).
- Re-entry: `set_event_max_reentries` allows a checked-in ticket to be passed
  back out with `pass_out` up to the limit, which the organizer sets per event.
  `reentry_count` reads the count (#78).
- `tickets_of(owner, start, limit)` pages through the ticket ids an address
  holds, with `limit` clamped to `MAX_PAGE_SIZE`. The index is kept current on
  mint, direct and batch transfers, gift claims, resale purchases and burns
  (#111). Count with `balance`, as the SEP-50 decision above describes.
- `burn_used_ticket(organizer, ticket_id)` deletes a checked-in ticket's record
  (and metadata URI) once `starts_at + USED_TICKET_RETENTION_SECONDS` (30 days)
  has passed. Revoked or unused tickets are rejected with `TicketNotUsed`, and
  early calls with `RetentionPeriodNotElapsed` (new codes 48 and 49) (#113).
- `TicketTransferred` event (`ticket_id` topic, `from`, `to`) published by
  `transfer_ticket`. Batch transfers, gift claims and resale purchases do not
  publish it yet (#115).
- `total_tickets()` returns the number of tickets ever issued across all
  events. It only grows, so revoked, used and burned tickets still count (#114).
- Optional ticket metadata URI set at issuance through
  `issue_ticket_with_metadata`, bounded by `MAX_METADATA_URI_LEN` (256 bytes),
  and read back through `token_uri`. `issue_ticket` keeps its signature and
  mints a ticket without a URI (#109).
- Soulbound events: `set_event_transferable(organizer, event_id, false)` makes
  an event's tickets non-transferable across direct transfers, batch transfers,
  gift claims and resale. The setting is locked once a ticket is issued. New
  error `NotTransferable` (#108).
- Paginated organizer event listing `list_organizer_events(organizer, start,
  limit)`, with `limit` clamped to `MAX_PAGE_SIZE` (50) (#112).
- SEP-50 read functions `owner_of`, `balance` and `token_uri`, backed by a
  per-owner ticket counter. The SEP-50 review, with its gaps, is in
  [docs/SEP50.md](docs/SEP50.md) (#110). The issue asked for `balance_of`, but
  SEP-50 names the function `balance`, so the contract follows the standard.
- Status-only scanner query `is_ticket_valid(ticket_id) -> Result<bool, Error>`
  with lifecycle, authorization-free read, and TTL tests. The existing
  owner-aware `is_valid(ticket_id, owner)` ABI is preserved (#350).
- Admin-managed organizer allowlist (`approve_organizer`,
  `revoke_organizer`, `is_approved_organizer`). `create_event` and
  `create_event_with_options` now reject callers that are not approved
  with `OrganizerNotApproved`, which stops event-id squatting (#135). See
  [docs/ORGANIZER_ALLOWLIST.md](docs/ORGANIZER_ALLOWLIST.md).
- Authorization test matrix covering every mutating entry point (#136).
- Initial `ticketing` contract: event registration, issuance, primary
  sale, transfer, on-chain verification, check-in, revocation, and a
  resale marketplace with anti-scalping price caps and organizer
  royalties.
