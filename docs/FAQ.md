# FAQ

Quick answers to common questions about how the ticketing contract behaves.
Each answer starts with the short version, followed by the details.

**Jump to:**
[One contract for all industries](#why-not-one-contract-per-industry) ·
[Changing the resale cap](#can-an-organizer-change-the-resale-cap-after-tickets-are-sold) ·
[Resale multiplier range](#what-is-the-accepted-range-for-max_resale_multiplier_bps) ·
[Text length limits](#what-are-the-byte-limits-on-the-name-category-tier-and-seat-strings) ·
[Compromised organizer key](#what-happens-to-a-ticket-if-the-organizer-account-is-compromised) ·
[Ticket limit per event](#is-there-a-maximum-number-of-tickets-per-event)

---

## Why not one contract per industry?

**Short answer:** Because the industry is just a label. The rules for
issuing, transferring and checking in tickets are the same everywhere, so
one contract covers concerts, conferences, sports and so on.

An event's `category` is stored as metadata. It does not switch on any
different logic. For the full reasoning, see "Why one contract for every
industry" in the root README.

---

## Can an organizer change the resale cap after tickets are sold?

**Short answer:** No.

Two settings control resale:

- `max_resale_multiplier_bps`: the highest price a ticket can be resold for,
  compared with its original price.
- `royalty_bps`: the share of each resale that goes back to the organizer.

Both are set once, when the event is created with `create_event`. They then
apply to every ticket under that event for as long as the event exists. This
protects buyers: the rules they bought under cannot change later.

> **What is "bps"?** It stands for basis points. 1 bps is 0.01%, so
> 100 bps = 1% and 10,000 bps = 100%.

---

## What is the accepted range for `max_resale_multiplier_bps`?

**Short answer:** 10,000 or higher.

| Value | Meaning | Allowed? |
|---|---|---|
| Below 10,000 | Cap is under face value | No, fails with `InvalidMultiplier` |
| 10,000 | Resale at face value only (100%) | Yes |
| 12,000 | Up to 20% above face value (120%) | Yes |

The 12,000 value is the one used throughout the docs and examples.

**Why is anything below 10,000 rejected?** The cap would sit below the price
the ticket was originally sold for, so no resale could ever meet it. The
contract refuses the setting up front instead of letting it create a ticket
that can never be resold.

**Worked example:** a ticket sold for 100 with a multiplier of 12,000 can be
resold for at most 100 × 12,000 / 10,000 = 120.

---

## What are the byte limits on the name, category, tier and seat strings?

**Short answer:** 64 bytes for `name`, 32 bytes for each of the others.

| Field | Maximum length | Can it be empty? |
|---|---|---|
| `name` | 64 bytes | No |
| `category` | 32 bytes | No |
| `tier` | 32 bytes | See note below |
| `seat` | 32 bytes | See note below |

Errors you may see:

- `EmptyNameOrCategory`: the `name` or `category` was empty.
- `StringTooLong`: a value was longer than its limit.

**Why limits exist:** the contract stores these strings on-chain, and storage
costs rent. Capping the length stops an organizer from running up very large
per-ticket storage costs.

> **Note on bytes vs. characters:** the limit is counted in bytes, not
> characters. Plain English letters take 1 byte each, but characters such as
> accented letters, emoji or non-Latin scripts can take 2 to 4 bytes. A label
> with fewer than 32 visible characters can still be too long.

> **Note on `tier` and `seat`:** an empty value is only rejected for `name`
> and `category`. The contract does not describe `tier` or `seat` emptiness as
> an error, so check `CONTRACT_API.md` if you need to be sure.

---

## What happens to a ticket if the organizer account is compromised?

**Short answer:** Anyone who holds the organizer's signing key can act as the
organizer, and the contract has no separate recovery path.

With that key, an attacker can, for that organizer's events:

- revoke tickets, and
- check tickets in.

The contract cannot tell the real organizer from someone using a stolen key,
because both produce a valid signature.

**What to do:** treat the organizer key like any other high-value signing key.
Keep it in secure storage, limit who can use it, and consider a multisig
account for important events.

---

## Is there a maximum number of tickets per event?

**Short answer:** Not by default. The contract does not set a hard limit on
its own.

The contract counts issued tickets in `tickets_issued`, a `u64` number, so the
only built-in ceiling is the size of a `u64`, which is far beyond any real
event.

If you need a limit (for example a venue with 5,000 seats), enforce it in
your own system, or use any capacity setting your deployment provides.