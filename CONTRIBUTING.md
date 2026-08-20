# Contributing to StellarTickets/blockchain

Thanks for considering a contribution to the `ticketing` contract.

## Development setup

```bash
rustup target add wasm32v1-none
cargo test -p stellar-tickets-ticketing
```

## Before opening a PR

- `cargo fmt --all`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --workspace`
- `stellar contract build` to confirm the wasm still builds

## Commit style

Keep commits scoped to one logical change. Prefer imperative subject
lines ("Add resale price cap test" not "Added" or "Adding").

## Reporting issues

Open a GitHub issue with a minimal reproduction — for contract bugs,
a failing test case is the most useful thing you can attach.
