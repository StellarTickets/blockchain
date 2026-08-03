# Fees

Every write to this contract is a regular Soroban transaction and
pays the network's standard resource fee — there is no separate
"platform fee" charged by the contract itself. The only value the
contract ever moves beyond the caller's own transaction fee is the
`payment_token` amounts in `purchase_primary` and `buy_resale`
(primary sale proceeds and resale royalty/seller proceeds).

Callers should simulate before submitting (`simulateTransaction` /
`prepareTransaction` in the SDK) to get an accurate resource fee for
the specific operation, since fees scale with the footprint touched —
`issue_ticket` and `buy_resale` touch more storage than a simple
`verify_ticket` read.
