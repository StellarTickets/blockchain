# Cap and royalty formulas with rounding examples

The contract uses integer arithmetic (basis points, denominator = 10_000) for
both the resale price cap and the organizer royalty split. Because division
truncates toward zero, the effective cap and royalty can be slightly lower
than the nominal percentage when the intermediate product is not evenly
divisible by 10_000.

This document gives concrete examples so integrators can verify their
off-chain calculations match on-chain behavior.

---

## Resale price cap

**Formula (on-chain):**

```rust
let cap = ticket.original_price * event.max_resale_multiplier_bps as i128 / 10_000;
```

- `original_price`: the price paid at primary sale (stored in `Ticket.original_price`)
- `max_resale_multiplier_bps`: per-event cap, e.g. 12_000 = 120%
- Division is integer division (truncates toward zero)

### Examples

| original_price | max_resale_multiplier_bps | exact product | cap (truncated) | effective % |
|----------------|---------------------------|---------------|-----------------|-------------|
| 1000           | 12000                     | 12_000_000    | 1200            | 120.00%     |
| 1001           | 12000                     | 12_012_000    | 1201            | 119.98%     |
| 1000           | 13333                     | 13_333_000    | 1333            | 133.30%     |
| 1000           | 10001                     | 10_001_000    | 1000            | 100.00%     |
| 1000           | 10000                     | 10_000_000    | 1000            | 100.00%     |
| 1              | 12000                     | 12_000        | 1               | 100.00%     |
| 1              | 20000                     | 20_000        | 2               | 200.00%     |

**Key observations:**

- The effective cap is **always ≤** the nominal percentage.
- For small `original_price` values, truncation can reduce the cap
  significantly (e.g., 1 stroop at 120% caps at 1 stroop = 100%).
- The contract rejects any `price > cap` in `list_for_resale`, so a
  listing at exactly `cap` is accepted.

### Off-chain replication (TypeScript)

```ts
function computeCap(originalPrice: bigint, maxResaleMultiplierBps: number): bigint {
    return (originalPrice * BigInt(maxResaleMultiplierBps)) / 10_000n;
}
```

---

## Organizer royalty on resale

**Formula (on-chain):**

```rust
let royalty = ticket.resale_price * event.royalty_bps as i128 / 10_000;
let seller_amount = ticket.resale_price - royalty;
```

- `resale_price`: the listing price set by the seller in `list_for_resale`
- `royalty_bps`: per-event royalty, e.g. 500 = 5%
- Division is integer division (truncates toward zero)
- `seller_amount` is computed as the remainder, so `royalty + seller_amount == resale_price` always holds

### Examples

| resale_price | royalty_bps | exact product | royalty (truncated) | seller_amount | effective % |
|--------------|-------------|---------------|---------------------|---------------|-------------|
| 1100         | 500         | 550_000       | 55                  | 1045          | 5.00%       |
| 1101         | 500         | 550_500       | 55                  | 1046          | 4.995%      |
| 1100         | 333         | 366_300       | 36                  | 1064          | 3.27%       |
| 1100         | 100         | 110_000       | 11                  | 1089          | 1.00%       |
| 1100         | 1           | 1_100         | 0                   | 1100          | 0.00%       |
| 10000        | 500         | 5_000_000     | 500                 | 9500          | 5.00%       |
| 10001        | 500         | 5_000_500     | 500                 | 9501          | 4.9995%     |

**Key observations:**

- The effective royalty is **always ≤** the nominal percentage.
- For small `resale_price` values or small `royalty_bps`, the royalty can
  round down to zero (e.g., 1100 at 1 bps → 0 royalty).
- `seller_amount` is **always** `resale_price - royalty`, so the split
  never exceeds the listing price and no value is lost to rounding.

### Off-chain replication (TypeScript)

```ts
function computeRoyaltySplit(resalePrice: bigint, royaltyBps: number): { royalty: bigint; sellerAmount: bigint } {
    const royalty = (resalePrice * BigInt(royaltyBps)) / 10_000n;
    const sellerAmount = resalePrice - royalty;
    return { royalty, sellerAmount };
}
```

---

## Floor price (optional)

**Formula (on-chain):**

```rust
if let Some(floor_bps) = event.min_resale_multiplier_bps {
    let floor = ticket.original_price * floor_bps as i128 / 10_000;
    if price < floor {
        return Err(Error::ResalePriceBelowFloor);
    }
}
```

Same truncation behavior as the cap: the effective floor is **always ≥**
the nominal percentage (because truncation reduces the computed floor,
making it easier to satisfy `price >= floor`).

### Example

| original_price | min_resale_multiplier_bps | exact product | floor (truncated) | effective % |
|----------------|---------------------------|---------------|-------------------|-------------|
| 1000           | 8000                      | 8_000_000     | 800               | 80.00%      |
| 1001           | 8000                      | 8_008_000     | 800               | 79.92%      |

---

## Summary for integrators

- **Always replicate the exact integer formula** off-chain:
  `price * bps / 10_000` with truncating division.
- **Never use floating point** — it will diverge from on-chain results.
- The contract's behavior is **deterministic and favors the seller/organizer
  slightly** (cap rounds down, royalty rounds down, floor rounds down).
- Test edge cases: `original_price = 1`, `royalty_bps = 1`, large values
  near `i128::MAX`.

---

## Related tests

- `resale_listing_rejects_prices_above_cap`
- `resale_price_exactly_at_the_face_value_cap_is_allowed`
- `resale_cap_boundary_with_rounding_truncates_down`
- `buy_resale_splits_royalty_and_transfers_ownership`
- `buy_resale_with_zero_royalty_pays_the_seller_in_full`
- `list_for_resale_after_cancel_succeeds_and_allows_purchase` (shows 57 royalty on 1150 = 5%)