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
- Releases attach the contract spec as `stellar_tickets_ticketing.spec.json`
  and `.spec.xdr`, exported by `scripts/export-spec.sh` (`make spec`) (#60).
- Sample event indexer in `examples/indexer` that decodes events from the
  release spec (#58).
- `stellar tickets` CLI plugin in `scripts/plugins` for admin and TTL
  maintenance calls, with a mainnet prompt and `--build-only` for multisig
  admins (#62).
- Mainnet monitoring guide, `docs/MONITORING.md` (#64).
- Primary-sale platform fee: `set_platform_fee_bps` takes a fee in basis points
  from each primary sale and pays it to the admin. The organizer receives the
  rest, through escrow when the event uses it. `platform_fee_bps` reads it
  (#104).
- Timed resale auctions: `start_auction`, `place_bid`, `settle_auction` and
  `get_auction`. Bids are escrowed, each bid refunds the bidder it displaces,
  and the highest bid wins at settlement. Bids stay within the resale price cap.
  While an auction runs, the ticket cannot be transferred, listed, sold at a
  fixed price or gifted. `cancel_resale` withdraws an auction with no bids
  (#105).
- Resale offers: `make_offer` escrows a buyer's offer on a listed ticket, and
  `accept_offer` settles it for the owner. `cancel_offer` lets the owner reject
  an offer or lets the buyer withdraw it. `get_offer` reads it (#106).
- Operator approvals: `approve`, `revoke_approval`, `get_approved` and
  `transfer_from` let one operator per ticket move it on the owner's behalf.
  Any change of owner clears the approval (#107).
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
