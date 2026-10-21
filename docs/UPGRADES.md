# Upgradeability

The `ticketing` contract has an admin-only upgrade path, `upgrade(admin,
new_wasm_hash)` (issue #101). It replaces the contract's code and keeps its
storage. Ticket ownership is a high-stakes invariant, and the admin key can
change the rules that apply to existing tickets, so every upgrade is an
explicit, reviewable act. It changes the trust model described in
[SECURITY.md](../SECURITY.md) and should be called out in the release notes.

## Upgrade procedure

1. Build the new WASM and upload it to the ledger, for example with
   `stellar contract upload`. Record the returned 32-byte hash.
2. Call `upgrade(admin, new_wasm_hash)` as the stored admin. The new code
   must already be uploaded, or the host rejects the call.
3. Check the result with `version()`, which reports the crate version of the
   build that is now running.
4. Run any migration entry point the new build requires (see below) before
   opening the contract to users.

`upgrade` publishes `ContractUpgraded` with the admin and the new hash. The
executable is replaced once the invocation succeeds, so a failed call leaves
the previous build in place.

## State migration rules

`upgrade` changes code only. Storage is never rewritten, so every build
after an upgrade must read the stored data the previous build wrote.

- **Storage keys.** Never remove, reorder, or change the meaning of an
  existing `DataKey` variant. Add new variants at the end of the enum only.
- **Stored structs.** `Event`, `Ticket`, `StoredTicket` and the other
  `#[contracttype]` records are encoded by field. Adding a field to a stored
  struct breaks decoding of existing entries. Store new data under a new
  `DataKey` instead, as the `SaleWindow`, `TierZones` and `OwnerHistory`
  entries do, so existing records stay readable.
- **Error codes.** Codes are append-only, as described in
  [ERRORS.md](ERRORS.md). A code keeps its meaning across builds.
- **Events.** Event shapes are part of the indexer contract
  ([EVENTS.md](EVENTS.md)). Changing a published event's fields needs a
  coordinated indexer change.
- **Migrations.** When a build must rewrite stored data, it ships an
  admin-only migration entry point. That entry point must be idempotent, so a
  retried call does nothing new. Run it right after the upgrade, and record
  the build version it was run against. Remove a migration entry point only
  after every record has been migrated.

## Testing an upgrade

The authorization matrix covers `upgrade`, and `test/upgrade.rs` checks that
a non-admin is rejected and that the admin can switch to a second WASM. A
new release that changes stored data should add a test that writes state with
the old layout and reads it with the new build.
