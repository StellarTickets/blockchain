# Audit preparation

This document defines the scope, build instructions, and known limitations for third-party security audits of the StellarTickets ticketing contract.

## Scope

The audit covers the core ticketing contract logic in [`contracts/ticketing/src/lib.rs`](../contracts/ticketing/src/lib.rs) and its modules:

- **Authorization** — `require_auth()` correctness for every state-changing entry point
- **State machine** — valid transitions between `TicketStatus` values (Valid → Used, Valid → Revoked, Valid → Resale, etc.)
- **Resale enforcement** — price cap validation (`max_resale_multiplier_bps`) and royalty splitting
- **Storage lifecycle** — ledger TTL management and eviction safety
- **Token interaction** — SEP-41 token contract calls for primary sales and resale settlement
- **Event isolation** — multi-organizer scenarios where different events may have conflicting IDs or royalty rules

### Out of scope

- Backend API logic (in the [`StellarTickets/backend`](https://github.com/StellarTickets/backend) repository)
- Frontend UX or wallet integration (in the [`StellarTickets/frontend`](https://github.com/StellarTickets/frontend) repository)
- Stellar network operation, validator consensus, or protocol-level security
- Third-party token contract implementations (e.g., Stellar Asset Contract vulnerabilities)

## Build instructions

**Prerequisites:**

- Rust 1.70.0 or later (MSRV pinned in [`rust-toolchain.toml`](../rust-toolchain.toml))
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- The [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools)

**Building the contract:**

```bash
# Clone the repository
git clone https://github.com/StellarTickets/blockchain.git
cd blockchain

# Run the full test suite (30+ unit tests with ~100% coverage of entry points)
cargo test -p stellar-tickets-ticketing

# Format check (must match CI)
cargo fmt --check

# Lint check (must pass with -D warnings)
cargo clippy --all-targets -- -D warnings

# Build the optimized WASM binary
stellar contract build
```

The compiled contract is output to:
```
target/wasm32v1-none/release/stellar_tickets_ticketing.wasm
```

Verify the binary matches the source via:
```bash
stellar contract install --source <testnet-key> --network testnet \
  target/wasm32v1-none/release/stellar_tickets_ticketing.wasm
# This returns a WAX hash; reproduce it on a clean build to verify determinism
```

## Test suite

The contract includes 30+ unit tests in [`contracts/ticketing/src/test.rs`](../contracts/ticketing/src/test.rs) covering:

- Happy path for every entry point (issue, transfer, check-in, revoke, resale)
- Authorization failures (non-organizer, non-owner, admin-only)
- State machine violations (double check-in, actions on revoked/used tickets)
- Resale boundary conditions (price cap enforcement, royalty rounding)
- Multi-event isolation (organizers don't interfere with each other's events)
- Fee and royalty splitting (primary sales, resales with organizer take)
- Ledger TTL/bumping (storage eviction safety)

Run locally:

```bash
cargo test -p stellar-tickets-ticketing -- --nocapture
```

All tests use `soroban_sdk::testutils` with `env.mock_all_auths()`, so no real network is needed; this is standard practice for Soroban unit testing.

## Known limitations

1. **Batch size** — `transfer_batch`, `check_in_batch`, and `revoke_batch` are limited to `MAX_BATCH_SIZE` (currently 100) entries per call, due to Soroban's storage I/O budgets. A single transaction cannot process more than this without hitting resource limits.

2. **Payment token decimals** — The contract assumes SEP-41 tokens have ≤ 18 decimal places. Tokens with more precision (unlikely on Stellar, but theoretically possible) may experience rounding in royalty splits.

3. **Price representation** — All monetary amounts are `i128` (Soroban's widest integer type). This limits the maximum ticket price to ~2^126 stroops (~10^37 XLM), which is far larger than any practical use case but is a theoretical ceiling. Negative prices are explicitly rejected.

4. **No upgrade mechanism** — The contract is not upgradeable. New features or bug fixes require a full redeployment to a new contract ID, and the backend must migrate all data via transfer transactions. Plan accordingly for the cost of data migration on mainnet.

5. **Fuzzing status** — The contract has not been subjected to formal fuzzing or model checking. Unit tests cover the main paths; property-based or symbolic execution testing was not part of the development process.

6. **Ledger TTL edge cases** — Storage entries expire 31 days after their last "bump" (extension). A long-running event with no transactions for 31+ days risks eviction of its event record. The contract will not restore or archive automatically; recovery requires manual re-creation or a migration by the organizer.

7. **XDR serialization** — The contract relies on Soroban's automatic XDR serialization for all storage. Any future upgrade to the Soroban SDK that changes serialization semantics could affect data layout, though this is unlikely given the stability of the Stellar ecosystem.

## Testnet soak period

Before any mainnet deployment, the contract should be deployed to testnet and exercised through a full ticket lifecycle with real users or integration tests:

1. Create an event
2. Issue and transfer tickets
3. Perform check-ins
4. List and buy resales (requires testnet SEP-41 token for settlement)
5. Revoke and refund scenarios

See [`DEPLOYMENT.md`](DEPLOYMENT.md) for testnet setup and walkthrough commands.

## Audit contact

Contact the StellarTickets team via [GitHub issues](https://github.com/StellarTickets/blockchain/issues) with findings or questions about the contract.
