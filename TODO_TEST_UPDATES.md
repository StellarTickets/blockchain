# Test Updates Required for purchase_primary Signature Change

## Overview
The `purchase_primary` function signature has changed to include a `max_price` parameter (issue #450).

**Old signature:**
```rust
pub fn purchase_primary(
    env: Env,
    buyer: Address,
    event_id: u64,
    tier: String,
    seat: String,
) -> Result<u64, Error>
```

**New signature:**
```rust
pub fn purchase_primary(
    env: Env,
    buyer: Address,
    event_id: u64,
    tier: String,
    seat: String,
    max_price: i128,  // NEW PARAMETER
) -> Result<u64, Error>
```

## Test Files That Need Updates

All test files that call `client.purchase_primary()` need to add the `max_price` parameter.
A sensible default for existing tests is `i128::MAX` (no slippage protection).

### Files requiring updates:
1. `contracts/ticketing/src/test/tickets.rs` (5+ calls)
2. `contracts/ticketing/src/test/events.rs` (5+ calls)
3. `contracts/ticketing/src/test/auth.rs` (4+ calls)
4. `contracts/ticketing/src/test/presale.rs` (3 calls)
5. `contracts/ticketing/src/test/escrow.rs` (2 calls)
6. `contracts/ticketing/src/test/ttl.rs` (1 call)
7. `contracts/ticketing/src/test/ticket_labels.rs` (3 calls)
8. `contracts/ticketing/src/test/auth_matrix.rs` (1 call)
9. `contracts/ticketing/src/test/cancel_event.rs` (1 call)
10. `contracts/ticketing/src/test/capacity.rs` (1 call)
11. `contracts/ticketing/src/test/primary_sale.rs` (1 call)
12. `contracts/ticketing/src/test/tier_capacity.rs` (1 call)

## Suggested Approach

For each call to `purchase_primary`, add `&i128::MAX` as the last parameter:

```rust
// Before:
client.purchase_primary(
    &buyer,
    &1,
    &String::from_str(&env, "GA"),
    &String::from_str(&env, "1"),
);

// After:
client.purchase_primary(
    &buyer,
    &1,
    &String::from_str(&env, "GA"),
    &String::from_str(&env, "1"),
    &i128::MAX,  // No slippage protection for most tests
);
```

## New Tests to Add

Add specific tests for issue #450 in `contracts/ticketing/src/test/tier_price.rs` or a new file:

1. Test that purchase_primary fails with `PriceChanged` when tier price > max_price
2. Test that purchase_primary succeeds when tier price <= max_price
3. Test a scenario where organizer calls `set_tier_price` between buyer's quote and purchase

Example:
```rust
#[test]
fn purchase_primary_rejects_price_increase_above_max() {
    let (env, client, _token, token_asset, _admin, organizer) = setup();
    make_event(&env, &client, &organizer, 1);
    let buyer = Address::generate(&env);
    token_asset.mint(&buyer, &10_000i128);
    
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &1_000i128);
    // Buyer quotes at 1_000 and sets max_price to 1_000
    // Organizer front-runs with price increase
    client.set_tier_price(&organizer, &1, &String::from_str(&env, "GA"), &2_000i128);
    
    // Purchase should fail with PriceChanged
    assert_eq!(
        client.try_purchase_primary(
            &buyer,
            &1,
            &String::from_str(&env, "GA"),
            &String::from_str(&env, "1"),
            &1_000i128,  // max_price from quote
        ),
        Err(Ok(Error::PriceChanged))
    );
}
```

## Running Tests

After updating all calls:
```bash
cd contracts/ticketing
cargo test
```
