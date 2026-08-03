# FAQ

**Why not one contract per industry?**
See "Why one contract for every industry" in the root README —
category is metadata, not a separate code path.

**Can an organizer change the resale cap after tickets are sold?**
No — `max_resale_multiplier_bps` and `royalty_bps` are set once in
`create_event` and apply to every ticket under that event for its
lifetime.

**What happens to a ticket if the organizer account is compromised?**
Whoever controls the organizer's signing key can revoke or check in
tickets for that event — the contract has no separate recovery path.
Treat the organizer key with the same care as any other high-value
signing key.

**Is there a maximum number of tickets per event?**
No hard cap in the contract; `tickets_issued` is a `u64` counter.
