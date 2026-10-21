# Example clients

Reference scripts that drive the ticketing contract end to end on the
Stellar **testnet** (issues #225, #226, #54 and #56): they issue one ticket
and then verify it, or connect a browser wallet / scanner app.

## Prerequisites

The contract must already be deployed to testnet, initialized and contain an
event. From the repository root, with the [stellar CLI] installed:

```bash
scripts/build.sh
scripts/deploy.sh <identity> testnet          # prints the contract ID, C...
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <identity> \
  --network testnet -- \
  initialize --admin <identity> --payment-token <TOKEN_CONTRACT_ID>
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <identity> \
  --network testnet -- \
  create_event --organizer <identity> --event-id 1 \
  --name "Radiohead Live" --category concert \
  --max-resale-multiplier-bps 12000 --royalty-bps 500 \
  --starts-at 1900000000 --transfer-freeze-seconds 0 --resale-cutoff-seconds 0
```

The examples below use event id `1`.

## TypeScript (`issue-and-verify.ts`, issue #225)

```bash
npm install @stellar/stellar-sdk tsx
CONTRACT_ID=C... npx tsx issue-and-verify.ts
```

Environment variables:

| Variable           | Meaning                                                       |
| ------------------ | ------------------------------------------------------------- |
| `CONTRACT_ID`      | required, the deployed contract address                       |
| `EVENT_ID`         | optional, defaults to `1`                                     |
| `ORGANIZER_SECRET` | optional; without it a funded testnet identity is generated   |
| `SOROBAN_RPC_URL`  | optional, defaults to the public testnet RPC                  |

The script mints one ticket with `issue_ticket` (signed by the organizer,
submitted to the network) and then reads it back with a simulated
`verify_ticket` call.

## Rust (`examples/stellar-cli-client`, issue #226)

The Rust example is a thin wrapper around the stellar CLI, so it has no SDK
dependencies of its own:

```bash
cargo run --manifest-path examples/stellar-cli-client/Cargo.toml -- \
  <CONTRACT_ID> <identity>
```

It submits `issue_ticket` with the given identity as organizer, then reads the
ticket back with a simulated `verify_ticket` (`--send no`).

## Browser wallet (`examples/freighter-wallet`, issue #54)

Connects a [Freighter] wallet, signs with the extension, and submits
`purchase_primary` on testnet. See that directory's README for setup.

```bash
cd examples/freighter-wallet && npm install && npm start
```

## Gate scanner (`examples/scanner-app`, issue #56)

Read-only scanner UI that verifies a ticket id via `get_ticket` /
`verify_ticket` and renders an admit / deny verdict. See that directory's
README for setup.

```bash
cd examples/scanner-app && npm install && npm start
```

[stellar CLI]: https://developers.stellar.org/docs/tools/cli/stellar-cli
[Freighter]: https://freighter.app
