# stellar-cli admin plugin

`stellar-tickets` wraps the contract's admin and maintenance calls so
operators do not retype `stellar contract invoke` with the contract ID,
network, source and admin arguments every time (issue #62).

The stellar CLI runs any executable named `stellar-<name>` on `PATH` as
`stellar <name>`, so adding this directory to `PATH` is the whole install:

```bash
export PATH="$PWD/scripts/plugins:$PATH"
stellar plugin ls        # lists "tickets"
stellar tickets help
```

## Configuration

Every option can come from the environment, so a shell set up once can run
many commands:

```bash
export STELLAR_TICKETS_CONTRACT_ID=C...
export STELLAR_NETWORK=testnet
export STELLAR_ACCOUNT=admin          # identity from `stellar keys ls`

stellar tickets status
```

| Option         | Environment variable          | Meaning                                    |
| -------------- | ----------------------------- | ------------------------------------------ |
| `--id`         | `STELLAR_TICKETS_CONTRACT_ID` | contract to call                           |
| `--network`    | `STELLAR_NETWORK`             | `testnet`, `futurenet` or `mainnet`        |
| `--source`     | `STELLAR_ACCOUNT`             | identity or key that signs                 |
| `--admin`      |                               | admin address; defaults to `--source`      |
| `--build-only` |                               | print unsigned XDR instead of submitting   |
| `--yes`        |                               | skip the mainnet confirmation prompt       |

## Commands

| Command                       | Calls                        | Notes                                                    |
| ----------------------------- | ---------------------------- | -------------------------------------------------------- |
| `status`                      | `is_paused`, `platform_fee_bps`, `total_tickets`, `token_decimals` | simulated, nothing is submitted |
| `is-organizer <address>`      | `is_approved_organizer`      | simulated                                                |
| `pause` / `unpause`           | `pause` / `unpause`          | admin only                                               |
| `approve-organizer <address>` | `approve_organizer`          | admin only, see [ORGANIZER_ALLOWLIST.md](../../docs/ORGANIZER_ALLOWLIST.md) |
| `revoke-organizer <address>`  | `revoke_organizer`           | admin only                                               |
| `set-fee <bps>`               | `set_platform_fee_bps`       | admin only, basis points (100 = 1%)                      |
| `set-throttle <ledgers>`      | `set_purchase_throttle`      | admin only, `0` disables the throttle                    |
| `propose-token <contract-id>` | `propose_payment_token`      | admin only, starts the payment-token time lock           |
| `apply-token`                 | `apply_payment_token`        | admin only, fails with `TimelockNotElapsed` until due    |
| `bump-ticket <ticket-id>...`  | `extend_ticket_ttl`          | any funded account, see [STORAGE_TTL.md](../../docs/STORAGE_TTL.md) |

Examples:

```bash
stellar tickets approve-organizer GORGANIZER...
stellar tickets set-fee 250
stellar tickets bump-ticket 12 13 14
```

## Mainnet and multisig admins

On `mainnet`, every submitting command prints the call and asks you to type
`mainnet` before it signs, like [`scripts/deploy.sh`](../deploy.sh). Pass
`--yes` only from reviewed automation.

When the admin is a multisig account (see
[`setup-multisig-admin.sh`](../setup-multisig-admin.sh)), no single key can
sign. Use `--build-only` with the admin's public address as `--source` to get
the unsigned transaction XDR, then collect signatures and submit it:

```bash
stellar tickets pause --network mainnet --source GADMIN... --build-only > pause.xdr
# --build-only skips simulation; simulate so the transaction carries the
# footprint and resource fee the network needs.
stellar tx simulate --network mainnet < pause.xdr > pause.sim.xdr
stellar tx sign --network mainnet --sign-with-key signer1 < pause.sim.xdr > pause.1.xdr
stellar tx sign --network mainnet --sign-with-key signer2 < pause.1.xdr > pause.2.xdr
stellar tx send --network mainnet < pause.2.xdr
```

All the underlying calls are documented with raw `stellar contract invoke`
examples in [`docs/STELLAR_CLI_EXAMPLES.md`](../../docs/STELLAR_CLI_EXAMPLES.md).
