# Freighter wallet integration (issue #54)

Browser-side example that connects a [Freighter] wallet, builds a Soroban
transaction against the `ticketing` contract, and lets the extension sign
and submit it on the Stellar **testnet**. No backend is required — the
wallet holds the keys and the page talks directly to the Soroban RPC.

## Prerequisites

1. The [Freighter browser extension] installed and unlocked.
2. A Freighter account funded on **testnet** (Freighter's network switcher
   must be set to *Testnet*).
3. A deployed, initialized contract with a created event and a priced tier —
   see [`scripts/examples/README.md`](../../scripts/examples/README.md)
   for the full setup.

## Files

| File | Purpose |
|---|---|
| `index.html` | Minimal UI: connect wallet, fetch address, invoke a read-only call |
| `src/main.js` | Freighter + Stellar SDK wiring |
| `package.json` | Dependencies (`@stellar/freighter-api`, `@stellar/stellar-sdk`) |

## Usage

```bash
cd examples/freighter-wallet
npm install
# serve the folder over HTTP (any static server works)
npx --yes serve .
# open http://localhost:3000 in a browser with Freighter installed
```

Set `CONTRACT_ID` and `EVENT_ID` in the page (or via `localStorage`) before
connecting. The example defaults to `EVENT_ID = 1`.

## What the flow does

1. **Connect** — `requestAccess()` prompts Freighter for the active
   account's public key. The page never sees the secret key.
2. **Read** — a simulated `get_event` call (via `server.simulateTransaction`
   with `--send no` semantics) loads the event so the UI can show its name
   and start time. Read-only calls do not need a signature.
3. **Write** — `purchase_primary` builds a `TransactionBuilder` envelope,
   hands it to `signTransaction(txXdr, { networkPassphrase })`, which
   pops Freighter's approval dialog, then submits the signed envelope with
   `server.sendTransaction` and polls `getTransaction` until it succeeds.

The signing step is the only place the user interacts with the extension.
Everything before it is ordinary RPC traffic; everything after is polling.

## Signing flow reference

```js
import { requestAccess, signTransaction } from "@stellar/freighter-api";
import {
  Address,
  Contract,
  Networks,
  BASE_FEE,
  TransactionBuilder,
  nativeToScVal,
  rpc,
} from "@stellar/stellar-sdk";

const server = new rpc.Server(RPC_URL);
const contract = new Contract(CONTRACT_ID);

// 1. get the public key (cached by Freighter after first approval)
const { address } = await requestAccess();

// 2. build an unsigned transaction
const tx = new TransactionBuilder(await server.getAccount(address), {
  fee: BASE_FEE,
  networkPassphrase: Networks.TESTNET,
})
  .addOperation(
    contract.call(
      "purchase_primary",
      new Address(address).toScVal(),
      nativeToScVal(EVENT_ID, { type: "u64" }),
      nativeToScVal("GA", { type: "string" }),
      nativeToScVal("unassigned", { type: "string" }),
    ),
  )
  .setTimeout(60)
  .build();

// 3. ask Freighter to sign; it returns the signed XDR as a string
const prepared = await server.prepareTransaction(tx);
const signedXdr = await signTransaction(prepared.toXDR(), {
  networkPassphrase: Networks.TESTNET,
});

// 4. submit the signed envelope
const signed = TransactionBuilder.fromXDR(signedXdr, Networks.TESTNET);
const sent = await server.sendTransaction(signed);
// poll server.getTransaction(sent.hash) until status !== "NOT_FOUND"
```

## Troubleshooting

| Symptom | Cause |
|---|---|
| `Freighter not installed` | The extension is missing or disabled for this origin |
| `User declined access` | The Freighter popup was closed without approving |
| Network mismatch errors | Freighter is on mainnet while the page builds testnet transactions — switch Freighter's network |
| `Account not funded` | The Freighter account has no testnet XLM; fund it from [Friendbot] |

## More documentation

- [`../../docs/INTEGRATION.md`](../../docs/INTEGRATION.md) — backend integration guide
- [`../../scripts/examples/issue-and-verify.ts`](../../scripts/examples/issue-and-verify.ts) — Node.js reference client (issue #225)
- [`../../examples/stellar-cli-client`](../stellar-cli-client) — CLI-based client (issue #226)

[Freighter]: https://freighter.app
[Freighter browser extension]: https://freighter.app
[Friendbot]: https://friendbot.stellar.org
