# Mainnet monitoring

What to measure, alert on and chart once the ticketing contract is live on
mainnet (issue #64). It covers the "Monitoring & observability" criteria in
[`MAINNET.md`](MAINNET.md#4-monitoring--observability).

The contract has no off-chain component of its own, so every signal comes
from one of three sources:

| Source | How to read it | Used for |
|---|---|---|
| Contract events | Soroban RPC `getEvents`, for example the [sample indexer](../examples/indexer/README.md) | activity, money flow, admin changes |
| Transaction results | RPC `getTransactions` / Horizon, filtered to calls into the contract | failed calls and their error codes |
| Contract state | simulated read calls (`stellar tickets status`, see [`scripts/plugins`](../scripts/plugins/README.md)) and RPC `getLedgerEntries` | pause flag, fee, TTLs |

Failed calls publish no events (Soroban rolls them back, see
[`ERRORS.md`](ERRORS.md)), so an events-only pipeline never sees errors. Watch
transaction results as well.

## Metrics

Derive these from the indexed events and transactions. Label every metric with
the contract ID so a redeploy to a new ID starts a new series.

### Activity

| Metric | Source | Notes |
|---|---|---|
| `events_created_total` | `EventCreated` | by `organizer` |
| `events_cancelled_total` | `EventCancelled` | by `event_id` |
| `tickets_issued_total` | `TicketIssued` | by `event_id`, `tier` |
| `tickets_checked_in_total` | `TicketCheckedIn` | by `organizer`; compare with issued per event |
| `tickets_transferred_total` | `TicketTransferred` | single transfers only, see [`EVENTS.md`](EVENTS.md#tickettransferred) |
| `resale_listings_open` | `TicketListed` minus `ListingCancelled` and `TicketResold` | gauge |
| `auctions_open` | `AuctionStarted` minus `AuctionSettled` | gauge; auctions withdrawn by `cancel_resale` need a state read |

### Money

All amounts are in the payment token's smallest unit; divide by
`token_decimals` for display. Per-event tokens (`event_payment_token`) mean a
single sum across events can mix tokens, so key amounts by token.

| Metric | Source |
|---|---|
| `primary_sales_volume` | sum of `PrimarySale.price` |
| `resale_volume` | sum of `TicketResold.price`, `OfferAccepted.amount`, `AuctionSettled.amount` |
| `royalties_paid` | sum of `TicketResold.royalty` |
| `offers_escrowed` | `OfferMade.amount` minus `OfferCancelled.amount` and `OfferAccepted.amount` |

### Health

| Metric | Source | Notes |
|---|---|---|
| `contract_calls_failed_total` | transaction results | by function and error code from [`ERRORS.md`](ERRORS.md) |
| `contract_paused` | `is_paused` | `pause`/`unpause` publish no event, so poll it |
| `platform_fee_bps` | `PlatformFeeUpdated`, or `platform_fee_bps` | |
| `instance_ttl_ledgers_left` | `getLedgerEntries` on the contract instance, `liveUntilLedgerSeq` minus the latest ledger | |
| `ticket_ttl_ledgers_left_min` | the same, for persistent `Ticket` entries of upcoming events | |
| `indexer_lag_ledgers` | latest ledger minus the last indexed event's ledger | |
| `rpc_errors_total` | the indexer's own RPC client | |

## Alerts

Severity: **page** wakes someone up, **ticket** is handled the next working
day. Thresholds are starting points; tune them after the testnet soak.

| Alert | Condition | Severity | First response |
|---|---|---|---|
| Unexpected admin change | any `PaymentTokenProposed`, `PaymentTokenChanged`, `PlatformFeeUpdated`, `OrganizerApproved`, `OrganizerRevoked` or `PurchaseThrottleUpdated` without a matching change ticket | page | confirm with the admin signers; a proposal can be overridden with a new one while the time lock runs |
| Pending payment-token change | `PaymentTokenProposed` seen | ticket | confirm it is planned and release escrow in the old token before `apply_payment_token` |
| Contract paused | `contract_paused` becomes `true` | page | confirm the pause was intended; all state-changing calls fail with `ContractPaused` |
| Authorization errors | `NotAdmin` or `NotOrganizer` failures above a low baseline | ticket, page on a spike | look for a misconfigured backend key or probing |
| Arithmetic or lifecycle errors | any `MathOverflow` or `InvalidTicketLifecycle` | page | these indicate a contract bug; collect the transaction and see [`SECURITY.md`](../SECURITY.md) |
| Payment failures | primary or resale purchases failing in the token contract | page | check the token contract and the buyer's trustline/balance; see [`NATIVE_XLM.md`](NATIVE_XLM.md) |
| Revocation spike | `revoke_ticket` / `revoke_batch` calls far above the usual rate for an event | ticket | confirm with the organizer |
| Instance TTL low | `instance_ttl_ledgers_left` below 100,000 (~6 days) | page | `stellar contract extend --id <contract-id> --durability persistent --ledgers-to-extend 535679 --network mainnet --source <account>`; simulated reads such as `stellar tickets status` do not extend it |
| Ticket TTL low | `ticket_ttl_ledgers_left_min` below 100,000 for an event that has not happened | ticket | `stellar tickets bump-ticket <ids>`; see [`STORAGE_TTL.md`](STORAGE_TTL.md) |
| Indexer behind | `indexer_lag_ledgers` above 120 (~10 minutes) | page | restart from the saved cursor before the RPC retention window passes |
| Check-in gap | `tickets_checked_in_total` flat during an event's check-in window | ticket | the venue scanners may be offline |

`LEDGER_THRESHOLD` (500,000 ledgers, ~29 days) is when the contract itself
refreshes a TTL on access. An entry falling well below it means nothing has
touched it, which is expected for quiet tickets but never for the instance.

## Dashboards

Three boards cover day-to-day operation.

**Overview** (for everyone)

- tickets issued, checked in and resold per hour
- primary and resale volume per token, and royalties paid
- open listings, open auctions, escrowed offers
- current pause flag, platform fee and payment token

**Operations** (for on-call)

- failed calls per hour, stacked by error code
- indexer lag and RPC errors
- lowest instance and ticket TTLs, in days
- timeline of admin events, annotated with change tickets

**Per event** (for organizer support)

- issued vs capacity vs checked in for one `event_id`
- resale price distribution against the event's resale cap
- revocations and refunds

## Runbook links

- Pausing and recovery: [`MAINNET.md`](MAINNET.md#rollback--recovery)
- Admin commands: [`scripts/plugins/README.md`](../scripts/plugins/README.md)
- Error meanings and retry semantics: [`ERRORS.md`](ERRORS.md)
- Event reference: [`EVENTS.md`](EVENTS.md)
