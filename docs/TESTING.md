# Testing

The test suite in
[`contracts/ticketing/src/test.rs`](../contracts/ticketing/src/test.rs)
uses `soroban_sdk::testutils` with `env.mock_all_auths()`, so every
`require_auth()` call succeeds without needing real signatures — this
is standard practice for Soroban unit tests and keeps focus on
contract logic rather than signature mechanics.

Coverage includes:

- Happy paths for every entry point (issue, purchase, transfer,
  check-in, revoke, list/cancel/buy resale)
- Authorization failures (wrong organizer, non-owner)
- State-machine violations (double check-in, revoked-ticket actions,
  listing/buying when not eligible)
- Boundary conditions (resale price cap, royalty bounds, negative and
  zero prices)
- Multi-event and multi-ticket scenarios

Run the full suite with:

```bash
cargo test -p stellar-tickets-ticketing
```
