# Scanner app example (issue #56)

Reference implementation of a gate-scanner flow: a small web app that
verifies a ticket by calling `verify_ticket` (or the preferred
`get_ticket`) against the deployed `ticketing` contract on the Stellar
**testnet**, so a venue staff member can check an attendee in without a
backend of their own.

A real scanner would pair this with a camera/QR input; the example uses a
text field so the flow is easy to follow and test.

## Prerequisites

1. A deployed, initialized contract with a created event and at least one
   issued ticket — see
   [`scripts/examples/README.md`](../../scripts/examples/README.md).
2. The ticket id to verify (printed by
   [`issue-and-verify.ts`](../../scripts/examples/issue-and-verify.ts) or
   the stellar CLI).
3. A funded testnet identity only if you also want to demo `check_in`;
   pure verification is read-only and needs no signature.

## Files

| File | Purpose |
|---|---|
| `index.html` | Minimal UI: ticket id input, Verify button, result panel |
| `src/main.js` | Builds a simulated `verify_ticket` / `get_ticket` call and decodes the result |
| `package.json` | Dependencies (`@stellar/stellar-sdk`) |

## Usage

```bash
cd examples/scanner-app
npm install
npx --yes serve .
# open http://localhost:3000, enter CONTRACT_ID and a ticket id, click Verify
```

## What the flow does

1. **Simulate** — build a `TransactionBuilder` envelope for
   `verify_ticket(ticket_id)` (deprecated alias of `get_ticket`) or
   `get_ticket(ticket_id)`, then call `server.simulateTransaction`. No
   signature and no fee payment are needed for a read-only call.
2. **Decode** — `scValToNative(sim.result[0].retval)` yields the `Ticket`
   struct: `event_id`, `owner`, `tier`, `seat`, `status`, pricing, and
   `transfers`.
3. **Decide** — the UI checks `status`:
   - `Valid` → admit (green)
   - `Used` → already checked in (yellow)
   - `Revoked` / `Refunded` → deny (red)
   - `Resale` → listed for resale; deny until bought or cancelled (yellow)
4. **Optional check-in** — if the scanner operator also holds the
   organizer (or a delegated check-in signer's) key, the app can submit
   `check_in` with `server.sendTransaction`. This is **not** in the demo
   by default because it requires signing; see the note below.

## Verify reference (read-only)

```js
import {
  Contract,
  Networks,
  BASE_FEE,
  TransactionBuilder,
  nativeToScVal,
  scValToNative,
  rpc,
} from "@stellar/stellar-sdk";

const server = new rpc.Server(RPC_URL);
const contract = new Contract(CONTRACT_ID);

async function verifyTicket(ticketId, sourceAddress) {
  const account = await server.getAccount(sourceAddress);
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      contract.call("get_ticket", nativeToScVal(BigInt(ticketId), { type: "u64" })),
    )
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (sim.error) throw new Error(sim.error);
  return scValToNative(sim.result[0].retval);
  // → { event_id, owner, tier, seat, status, original_price, resale_price, transfers, purchased_at }
}
```

`verify_ticket` is a deprecated alias kept for ABI compatibility — new
code should call `get_ticket`. Both return the same `Ticket` struct.

## Optional: submitting check-in from the scanner

Check-in is a **write** and must be signed by the event's organizer or a
delegated check-in signer (issue #89). The scanner app can do this when
the gate device holds the signer's key (via Freighter, an HSM, or a
backend signer service):

```js
import { signTransaction } from "@stellar/freighter-api";

const checkInTx = new TransactionBuilder(account, {
  fee: BASE_FEE,
  networkPassphrase: Networks.TESTNET,
})
  .addOperation(
    contract.call(
      "check_in",
      new Address(signerAddress).toScVal(),
      nativeToScVal(BigInt(ticketId), { type: "u64" }),
    ),
  )
  .setTimeout(30)
  .build();

const prepared = await server.prepareTransaction(checkInTx);
const signed = await signTransaction(prepared.toXDR(), {
  networkPassphrase: Networks.TESTNET,
});
const sent = await server.sendTransaction(
  TransactionBuilder.fromXDR(signed.signedTxXdr, Networks.TESTNET),
);
```

`check_in_at_zone` adds a zone argument for events that restrict tiers to
entry zones (issue #80); it returns `ZoneNotAllowed` when the ticket's
tier may not enter the scanned zone.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `TicketNotFound` (error code 5) | The ticket id does not exist, or its persistent entry has been archived (see [`STORAGE_TTL.md`](../../docs/STORAGE_TTL.md)) |
| Storage / archival host error before a contract code | The ticket's TTL has lapsed — restore the entry, then retry |
| `NotOrganizer` (error code 6) on check-in | The signing address is neither the organizer nor a delegated check-in signer |
| `ResaleListingActive` (error code 42) | The ticket is listed for resale and cannot be checked in until the listing is cancelled |
| `AlreadyUsed` (error code 8) | The ticket was already checked in |

## More documentation

- [`../../docs/CONTRACT_API.md`](../../docs/CONTRACT_API.md) — function reference for `verify_ticket`, `get_ticket`, `check_in`
- [`../../docs/ERRORS.md`](../../docs/ERRORS.md) — full error table
- [`../../scripts/examples/issue-and-verify.ts`](../../scripts/examples/issue-and-verify.ts) — Node.js issue-and-verify client (issue #225)
- [`../../examples/freighter-wallet`](../freighter-wallet) — browser wallet signing flow (issue #54)
