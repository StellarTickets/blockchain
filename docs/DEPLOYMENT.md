# Deployment checklist

1. `cargo test --workspace` — full suite green
2. `cargo clippy --all-targets -- -D warnings` — no lint warnings
3. `stellar contract build` — optimized wasm builds cleanly
4. Deploy to **testnet** first; run through the full ticket lifecycle
   manually (create event, issue, verify, check in, list, buy resale)
   against a real testnet SEP-41 token
5. Only after a testnet soak period, deploy to **mainnet** with a
   production payment token and a hardware-backed admin key
6. Record the deployed contract ID in the backend's
   `TICKETING_CONTRACT_ID` environment variable

## Release assets

Pushing a `v*` tag runs `.github/workflows/release.yml`, which attaches these
files to the GitHub release (and uploads them as the `ticketing-wasm` CI
artefact):

| File | Contents |
|---|---|
| `stellar_tickets_ticketing.wasm` | the contract to deploy |
| `stellar_tickets_ticketing.wasm.sha256` | its checksum |
| `stellar_tickets_ticketing.spec.json` | contract spec (functions, types, errors, events) as JSON (issue #60) |
| `stellar_tickets_ticketing.spec.xdr` | the same spec as base64 `SCSpecEntry` XDR |

Integrators can generate bindings or decode events from the spec without
building the wasm. `make spec` writes the same two spec files locally, and
for a deployed contract the spec can be checked against the release with
`stellar contract info interface --contract-id <id> --network <network> --output json-formatted`.

## Multisig admin

On mainnet the contract admin should be a multisig account rather than a
single key. Configure it before calling `initialize`:

```
scripts/setup-multisig-admin.sh <admin-identity> <network> <threshold> <signer-address>...
```

Run it on testnet first and confirm an admin call (for example
`set_purchase_throttle`) needs `<threshold>` signatures before doing the same
on mainnet.

## deploy.sh safety prompts (issue #228)

`scripts/deploy.sh <identity> <network>` guards against accidental use:

- **Dry run** — pass `--dry-run` to print the unsigned deployment
  transaction (base64 XDR, via `stellar contract deploy --build-only`)
  without submitting anything. Run it first to sanity-check the identity,
  network and wasm before a real deploy on any network:

  ```
  scripts/deploy.sh <identity> mainnet --dry-run
  ```

- **Mainnet confirmation** — a real mainnet deploy pauses and requires
  typing `confirm` at the prompt. Testnet/futurenet deployments submit
  without a prompt.

- **Scripted deploys** — pass `--yes` to skip the mainnet confirmation
  once the deployment has been reviewed (e.g. in automation).

The wasm path, identity and network are unchanged from the original script.
