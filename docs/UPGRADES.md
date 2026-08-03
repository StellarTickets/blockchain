# Upgradeability

The `ticketing` contract as deployed has no upgrade mechanism — Soroban
contracts are immutable once deployed unless explicitly built with an
upgrade hook. This is intentional for now: ticket ownership is a
high-stakes invariant, and an upgradeable contract widens the trust
assumptions users have to make (an upgrade key could rewrite the rules
retroactively).

If an upgrade path is added later, it should be a separate, explicit
proposal — not a silent addition — since it changes the security model
described in [SECURITY.md](../SECURITY.md).
