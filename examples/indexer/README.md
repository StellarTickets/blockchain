# Sample event indexer

A minimal indexer for the ticketing contract (issue #58). It polls Soroban
RPC `getEvents` for the contract and prints every event as one JSON line,
decoded into named fields. Pipe the output into a queue, a database loader or
`jq`; it is a starting point, not a production service.

The indexer does not hard-code any event layouts. It reads them from the
contract spec JSON that every release publishes
(`stellar_tickets_ticketing.spec.json`, see
[`scripts/export-spec.sh`](../../scripts/export-spec.sh)), so a release that
adds an event only needs the new spec file.

## Run it

```bash
# Build the wasm and export the spec locally, or download the spec JSON
# from the release that matches the deployed contract.
make spec

cd examples/indexer
npm install @stellar/stellar-sdk tsx
CONTRACT_ID=C... npx tsx indexer.ts
```

| Variable          | Meaning                                                                       |
| ----------------- | ----------------------------------------------------------------------------- |
| `CONTRACT_ID`     | required, the deployed contract address                                       |
| `SPEC_PATH`       | optional, path to the spec JSON; defaults to the one `make spec` writes       |
| `START_LEDGER`    | optional, first ledger to read; defaults to the oldest ledger the RPC retains |
| `SOROBAN_RPC_URL` | optional, defaults to the public testnet RPC                                  |
| `POLL_MS`         | optional, delay between polls when there are no new events (default `5000`)   |

Sample output for a ticket mint:

```json
{"id":"0000644245094400-0000000001","ledger":150,"ledgerClosedAt":"2026-10-20T00:00:00Z","txHash":"ab…","event":"TicketIssued","fields":{"ticket_id":"7","event_id":"1","owner":"GA55…","tier":"VIP"}}
```

`u64` and `i128` values are printed as strings so they survive JSON.

## How events are encoded

Every event is declared with `#[contractevent]` in
[`contracts/ticketing/src/events.rs`](../../contracts/ticketing/src/events.rs)
and is published as:

- **topics**: the event name in `snake_case` as a `Symbol` (for example
  `ticket_issued`), followed by each `#[topic]` field in order.
- **data**: a map of the remaining fields, keyed by field name.

So `TicketIssued` arrives as topics `["ticket_issued", ticket_id]` and data
`{ event_id, owner, tier }`. The spec JSON describes the same thing per event
(`prefix_topics`, each param's `location`, and `data_format`), which is what
`indexer.ts` uses to decode. The field reference is in
[`docs/EVENTS.md`](../../docs/EVENTS.md).

To follow one event type only, add a topic filter to the `getEvents` call,
for example `topics: [[<base64 XDR of Symbol "ticket_resold">, "*"]]`.

## Things a production indexer must handle

- **Retention.** Public RPCs keep only a recent window of events. Start the
  indexer before the contract goes live, persist the `cursor` after each
  page, and resume from it. A gap older than the retention window cannot be
  refilled from RPC and needs a history archive or a hosted indexer.
- **Idempotency.** Use the event `id` as the primary key; it is unique and
  ordered, so replays after a restart are harmless.
- **Ownership is not fully evented.** Batch transfers, gift claims, accepted
  offers and settled auctions do not publish `TicketTransferred` (see
  [`docs/EVENTS.md`](../../docs/EVENTS.md)). Track owners from
  `TicketIssued`, `TicketTransferred`, `TicketResold`, `OfferAccepted` and
  `AuctionSettled`, and reconcile with `get_ticket` / `owner_of`.
- **Spec versions.** Decode with the spec of the release that is deployed;
  the spec is attached to each GitHub release next to the wasm.

A table layout that works for most consumers:

```sql
CREATE TABLE contract_events (
  id               TEXT PRIMARY KEY,   -- RPC event id
  ledger           INTEGER NOT NULL,
  ledger_closed_at TIMESTAMPTZ NOT NULL,
  tx_hash          TEXT NOT NULL,
  event            TEXT NOT NULL,      -- e.g. TicketIssued
  fields           JSONB NOT NULL
);
CREATE INDEX ON contract_events (event, ledger);
```
