# Mainnet readiness

This document defines the requirements for deploying the StellarTickets ticketing contract to Stellar mainnet.

## Readiness criteria

Mainnet deployment is **blocked** until all of the following are satisfied:

### 1. Security audit ✗

- [ ] Third-party security audit completed and reviewed
- [ ] All **critical** and **high** findings remediated
- [ ] Medium/low findings documented with risk acceptance or fixes
- [ ] Audit report available to stakeholders (link below)

**Audit status:** pending (issue #183 tracks this)

**What to audit:** See [`AUDIT.md`](AUDIT.md) for scope, build instructions, and test coverage.

### 2. Code quality & testing ✓

- [x] 30+ unit tests covering all entry points and error cases (in `contracts/ticketing/src/test.rs`)
- [x] `cargo test` passes locally and in CI
- [x] `cargo clippy` and `cargo fmt` pass locally and in CI
- [x] WASM build is reproducible (`stellar contract build`)

All of these are enforced in GitHub Actions on every push to `main`.

### 3. Fuzzing

- [ ] Property-based fuzzing of state transitions (e.g., `proptest` on valid/invalid event IDs, royalty values)
- [ ] Differential fuzzing of resale price caps against reference implementation
- [ ] Corpus of generated test cases retained for regression testing

**Status:** Not started. Fuzzing is a future enhancement (e.g., via Soroban SDK fuzzing support when available).

### 4. Monitoring & observability

- [ ] Event indexer deployed (e.g., `TicketIssued` and `TicketCheckedIn` subscriptions)
- [ ] Monitoring dashboard for contract state (event count, tickets issued/used/revoked, resale volume)
- [ ] Alerts configured for unusual patterns (e.g., high revocation rate, royalty settlement failures)
- [ ] Data reconciliation job in backend validates on-chain and database state match (see [`backend`](https://github.com/StellarTickets/backend) repo)

**What to monitor:**
- Ledger TTL evictions (events expiring from storage)
- Token contract failures during primary sales or resales
- Authorization or state-machine errors (which should be rare or zero)

### 5. Testnet soak & integration

- [ ] Contract deployed to testnet and running for at least 2 weeks with real or synthetic load
- [ ] Full ticket lifecycle tested end-to-end (issue, transfer, check-in, revoke, resale)
- [ ] Backend integration tested against testnet contract (see backend's `TICKETING_CONTRACT_ID` env var)
- [ ] Frontend integration tested against testnet contract via Freighter or other browser wallet
- [ ] Multisig admin tested on testnet (see [`DEPLOYMENT.md`](DEPLOYMENT.md#multisig-admin) for setup)

### 6. Operational readiness

- [ ] Mainnet admin key is hardware-backed (e.g., Ledger, not a hot key)
- [ ] Admin multisig configured with 3+ signers and 2+ threshold on mainnet
- [ ] Runbook for emergency revoke/refund procedures documented
- [ ] Response plan for critical bugs (redeployment + data migration process)
- [ ] SLAs and incident escalation paths defined

### 7. Legal & compliance

- [ ] Terms of service and privacy policy updated to disclose on-chain storage
- [ ] Audit findings reviewed by legal team
- [ ] Compliance with relevant regulations (GDPR/CCPA for ticket data, financial regulations for resale)

**Status:** Out of scope for this repository; coordinate with StellarTickets business team.

## Deployment checklist

Once all readiness criteria are met:

```bash
# 1. Verify CI is green on main
git log -1 --oneline main

# 2. Build locally and sanity-check the WASM
stellar contract build
ls -lh target/wasm32v1-none/release/stellar_tickets_ticketing.wasm

# 3. Ensure mainnet admin key is backed up and available
stellar keys ls --network mainnet

# 4. Do a dry run first to see the unsigned transaction
scripts/deploy.sh <admin-identity> mainnet --dry-run

# 5. Review the XDR and proceed with the real deploy
# (you'll be prompted to confirm)
scripts/deploy.sh <admin-identity> mainnet

# 6. Record the contract ID
# The script will print a contract ID starting with "C"
# Save this as TICKETING_CONTRACT_ID in the backend's .env

# 7. Initialize the contract with the mainnet payment token
stellar contract invoke \
  --id <contract-id> \
  --source <admin-identity> \
  --network mainnet \
  -- initialize --admin <admin-address> --payment_token <mainnet-token-id>

# 8. Perform a sanity check: create an event and read it back
stellar contract invoke --id <contract-id> --source <admin-identity> \
  --network mainnet -- create_event \
  --organizer <organizer-address> --event_id 1 \
  --name '"First Mainnet Event"' --category '"test"' \
  --max_resale_multiplier_bps 12000 --royalty_bps 500

stellar contract invoke --id <contract-id> --network mainnet \
  -- get_event --event_id 1
```

## Rollback & recovery

**If a critical bug is discovered on mainnet:**

1. Do NOT call `revoke_ticket` on all tickets (this is a manual, expensive operation)
2. Instead, deploy a new contract to a new ID
3. Update `TICKETING_CONTRACT_ID` in the backend to point to the new contract
4. Reissue affected tickets on the new contract (backend migration job)
5. Document the incident and root cause

There is no pause or upgrade mechanism; contracts are immutable once deployed.

## References

- [`AUDIT.md`](AUDIT.md) — Audit scope and build instructions
- [`DEPLOYMENT.md`](DEPLOYMENT.md) — Testnet setup, dry-run, and multisig configuration
- [`TESTING.md`](TESTING.md) — Unit test coverage and running locally
- [`README.md`](../README.md) — Project overview and quick-start
