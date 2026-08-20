# StellarTickets — Blockchain

Soroban smart contracts powering [StellarTickets](https://github.com/StellarTickets) —
*Secure. Verifiable. Powered by Stellar.*

Every ticket issued through the platform is minted as an on-chain asset on the
Stellar network via the `ticketing` contract in this repository. This is the
single source of truth for ticket ownership, validity, and resale — the
[backend](https://github.com/StellarTickets/backend) reads and writes through
this contract rather than keeping its own notion of ownership, and the
[frontend](https://github.com/StellarTickets/frontend) never talks to it
directly (it goes through the backend and a browser wallet).

## Table of contents

- [New to this stack? Start here](#new-to-this-stack-start-here)
- [Why one contract for every industry](#why-one-contract-for-every-industry)
- [How this fits with the other repos](#how-this-fits-with-the-other-repos)
- [Contract: `ticketing`](#contract-ticketing)
- [Data model](#data-model)
- [Errors](#errors)
- [Getting started](#getting-started)
- [Testnet deployment walkthrough](#testnet-deployment-walkthrough)
- [Project structure](#project-structure)
- [More documentation](#more-documentation)

## New to this stack? Start here

A plain-language glossary for anyone who hasn't worked with Stellar/Soroban
before. Skip this if you already know the stack.

| Term | What it means | Why it matters here |
|---|---|---|
| **Stellar** | A public blockchain network optimized for fast (~5s), cheap payments and asset issuance. | It's the ledger every ticket, event, and payment in this project ultimately lives on. |
| **Soroban** | Stellar's smart contract platform. Contracts are written in Rust, compiled to WebAssembly (WASM), and run deterministically on every validator. | The `ticketing` contract in this repo *is* a Soroban contract — this is where "issue a ticket" actually becomes an unforgeable on-chain fact. |
| **Smart contract** | Code that runs on the blockchain itself, so its logic and state can't be changed by any single party (including us) without going through the same rules everyone else does. | This is why a ticket's validity can't be silently edited in a database — the contract is the referee. |
| **WASM (WebAssembly)** | A compact, sandboxed binary format. Soroban contracts compile down to it. | `stellar contract build` in this repo produces a `.wasm` file — that's the literal thing that gets deployed. |
| **Testnet / Futurenet / Mainnet** | Three separate Stellar networks: testnet (free play-money, safe to break things), futurenet (bleeding-edge features), mainnet (real money, real users). | You develop and test against testnet; StellarTickets only goes to mainnet once it's ready for real events. |
| **XDR** | Stellar's binary serialization format for transactions. An "unsigned XDR" is a transaction that's been built but not yet authorized by anyone. | The backend builds XDR for every on-chain write; see [non-custodial by design](https://github.com/StellarTickets/backend#non-custodial-by-design) in the backend repo for the full signing flow. |
| **`require_auth()`** | A Soroban SDK call that says "this specific address must have cryptographically signed this transaction, or it fails." | Every function below that changes state calls this on the relevant party (organizer, ticket owner, buyer) — it's what makes `NotOwner`/`NotOrganizer` checks actually enforceable, not just a suggestion. |
| **Basis points (bps)** | A unit equal to 1/100th of a percent. 100 bps = 1%, 10,000 bps = 100%. | `royalty_bps` and `max_resale_multiplier_bps` are both expressed this way — e.g. `royalty_bps: 500` means a 5% royalty on every resale. |
| **SEP-41 token** | Stellar's standard interface for fungible tokens (an ERC-20 equivalent), implemented by both the native XLM asset and custom Stellar Asset Contracts. | The `payment_token` this contract is initialized with is any SEP-41 token — primary sales and resale settlement move that token, not XLM specifically. |
| **Ledger TTL / "bump"** | Soroban storage isn't permanent by default — each entry has a time-to-live measured in ledgers, and has to be periodically extended ("bumped") or it gets archived. | Every write in this contract calls `extend_ttl` so events and tickets don't silently expire from storage; see `LEDGER_BUMP`/`LEDGER_THRESHOLD` in `lib.rs`. |
| **`stellar` CLI** | The official command-line tool for building, deploying, and invoking Soroban contracts. | Every command in this README is run through it. |

## Why one contract for every industry

Concerts, flights, sports, festivals, conferences, buses, movie theaters,
museums, tourist attractions, public transport, universities, and corporate
events all reduce to the same primitive: an **event** (or route, showing,
session) that issues **tickets** which must be verifiable, transferable, and
optionally resold under organizer-defined rules. Rather than one contract per
industry, `Event.category` carries the industry as free-text metadata, and
`Event.max_resale_multiplier_bps` / `royalty_bps` let each organizer tune
anti-scalping and resale-royalty policy per event — a bus operator might set
a 100% cap (no markup at all) while a concert promoter allows 120% with a 5%
royalty back to themselves.

Keeping `category` as a plain string (rather than a Rust enum) means adding a
13th industry is a backend/frontend change, not a contract redeploy.

## How this fits with the other repos

```text
┌─────────────────────┐      confirm-*      ┌──────────────────────┐
│      frontend        │ ───────────────────▶│       backend         │
│  (Next.js, browser)  │                      │  (NestJS + Postgres)  │
│                       │◀──── build-*XDR ─────│                       │
└──────────┬────────────┘                      └───────────┬───────────┘
           │  Freighter signs the XDR                       │
           │  client-side, never sends a key                │  submits signed XDR
           ▼                                                 ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │                  this repo: `ticketing` Soroban contract          │
   │            (Stellar network — the source of truth for            │
   │             ticket ownership, validity, and resale)               │
   └─────────────────────────────────────────────────────────────────┘
```

The backend keeps a Postgres copy of event/ticket data for fast search and
listing (see its `Ticket.status` cache), but on any disagreement between the
database and this contract, **the contract wins** — that's the whole point of
putting ticket ownership on-chain instead of only in a database a platform
operator could edit.

## Contract: `ticketing`

Path: [`contracts/ticketing`](contracts/ticketing)

| Function | Who must sign | Purpose |
|---|---|---|
| `initialize(admin, payment_token)` | `admin` | One-time setup: sets the platform admin and the SEP-41 token used for on-chain settlement. Fails if already called. |
| `create_event(organizer, event_id, name, category, max_resale_multiplier_bps, royalty_bps)` | `organizer` | Registers an event/route/showing. `royalty_bps` must be ≤ 10,000 (100%). `event_id` is chosen by the caller (typically a ULID cast to `u64`) so it can be correlated with the backend's own event row. |
| `issue_ticket(organizer, event_id, to, tier, seat, price)` | `organizer` | Organizer-authorized mint for a ticket already paid for off-chain (card payment, comp ticket, fiat settled by the platform). `price` may be `0` for comps. |
| `purchase_primary(buyer, event_id, tier, seat, price)` | `buyer` | Fully on-chain primary sale — the buyer pays the organizer directly in `payment_token`, then the ticket mints atomically in the same transaction. `price` may be `0` for free events. |
| `transfer_ticket(from, ticket_id, to)` | `from` | Direct ownership transfer (gift, family member). Blocked on used/revoked tickets; clears any active resale listing. |
| `verify_ticket(ticket_id)` | *(none — read-only)* | Looks up a ticket's current owner and status. The fraud-prevention primitive any gate scanner or app calls before admitting entry. |
| `check_in(organizer, ticket_id)` | `organizer` | Marks a ticket used at the point of entry. Emits `TicketCheckedIn`. Cannot be replayed — a second call on the same ticket fails with `AlreadyUsed`. |
| `revoke_ticket(organizer, ticket_id)` | `organizer` | Voids a ticket (chargeback, counterfeit, policy violation). Permanent — a revoked ticket can never be transferred, resold, or checked in again. |
| `list_for_resale(owner, ticket_id, price)` | `owner` | Lists a valid ticket for resale. `price` is capped at `original_price * max_resale_multiplier_bps / 10000` — the anti-scalping enforcement point. |
| `cancel_resale(owner, ticket_id)` | `owner` | Pulls a listing, returning the ticket to `Valid`. |
| `buy_resale(buyer, ticket_id)` | `buyer` | Buys a listed ticket. Settles atomically: the organizer's royalty cut transfers first, the remainder to the seller, then ownership flips to the buyer — all or nothing, in one transaction. |
| `get_event(event_id)` / `get_ticket(ticket_id)` | *(none — read-only)* | Raw storage lookups; `EventNotFound`/`TicketNotFound` on a miss. |

**Events emitted:** `TicketIssued { ticket_id, event_id }` (on every mint, via
`issue_ticket` or `purchase_primary`) and `TicketCheckedIn { ticket_id,
organizer }` (on `check_in`). Indexers and the backend's reconciliation job
subscribe to these instead of polling every ticket on every block — see
[`docs/EVENTS.md`](docs/EVENTS.md).

See [`contracts/ticketing/src/lib.rs`](contracts/ticketing/src/lib.rs) for the
full implementation and
[`contracts/ticketing/src/test.rs`](contracts/ticketing/src/test.rs) for the
behavioral spec — 30 tests covering issuance, check-in idempotency,
revocation, resale price caps, royalty splitting, multi-organizer isolation,
and free/comp-ticket edge cases.

## Data model

```text
Event
├── organizer: Address           the Stellar account that created this event
├── name: String
├── category: String             free text — "concert", "flight", "bus", etc.
├── max_resale_multiplier_bps: u32   anti-scalping cap, relative to original_price
├── royalty_bps: u32              organizer's cut of every resale, ≤ 10,000
└── tickets_issued: u64

Ticket
├── event_id: u64
├── owner: Address
├── tier: String                  e.g. "GA", "VIP", "Business"
├── seat: String                  e.g. "12A", or "unassigned" for GA
├── status: TicketStatus          Valid | Used | Revoked | Resale
├── original_price: i128          face value paid at issuance/primary sale
└── resale_price: i128            0 unless status == Resale
```

`i128` is used for all monetary amounts because Soroban tokens (including the
native XLM asset) use 7 decimal places of precision internally — a plain
`u64`/`i64` isn't wide enough for large amounts at that precision.

## Errors

All public functions return `Result<T, Error>`; `Error` is a `#[contracterror]`
enum, so callers get a typed, stable error code rather than a panic string:

| Code | Error | Meaning |
|---|---|---|
| 1 | `AlreadyInitialized` | `initialize` called more than once |
| 2 | `NotInitialized` | Contract used before `initialize` |
| 3 | `EventNotFound` | No event with that `event_id` |
| 4 | `EventAlreadyExists` | `create_event` called twice with the same `event_id` |
| 5 | `TicketNotFound` | No ticket with that `ticket_id` |
| 6 | `NotOrganizer` | Caller isn't the event's organizer |
| 7 | `NotOwner` | Caller isn't the ticket's current owner |
| 8 | `AlreadyUsed` | Action attempted on a checked-in ticket |
| 9 | `Revoked` | Action attempted on a revoked ticket |
| 10 | `NotForResale` | `cancel_resale`/`buy_resale` on a ticket that isn't listed |
| 11 | `ResalePriceExceedsCap` | Listing price exceeds `max_resale_multiplier_bps` |
| 12 | `InvalidPrice` | Negative price, or zero on a listing that requires > 0 |
| 13 | `InvalidRoyalty` | `royalty_bps` above 10,000 (100%) |

## Getting started

**Prerequisites:**

- [Rust](https://rustup.rs/) (stable channel — pinned via [`rust-toolchain.toml`](rust-toolchain.toml))
- The `wasm32v1-none` target: `rustup target add wasm32v1-none`
- The [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools) (`stellar`)

```bash
# clone and enter the repo
git clone https://github.com/StellarTickets/blockchain.git
cd blockchain

# run the full test suite (30 tests, no network required)
cargo test -p stellar-tickets-ticketing

# format + lint, matching CI
cargo fmt --check
cargo clippy --all-targets -- -D warnings

# build the optimized wasm binary
stellar contract build
```

The build output lands at
`target/wasm32v1-none/release/stellar_tickets_ticketing.wasm`.

## Testnet deployment walkthrough

```bash
# 1. Create and fund a testnet identity (this becomes the admin/organizer)
stellar keys generate organizer --network testnet --fund

# 2. Deploy the built wasm — this returns a contract ID (starts with "C")
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_tickets_ticketing.wasm \
  --source organizer \
  --network testnet

# 3. Initialize it against a testnet SEP-41 token (the native XLM contract
#    works fine for testing — look it up with `stellar contract id asset`)
stellar contract invoke \
  --id <contract-id> \
  --source organizer \
  --network testnet \
  -- initialize --admin <admin-address> --payment_token <token-contract-id>

# 4. Sanity-check it: create an event and read it back
stellar contract invoke --id <contract-id> --source organizer --network testnet \
  -- create_event --organizer <admin-address> --event_id 1 --name '"Test Show"' \
     --category '"concert"' --max_resale_multiplier_bps 12000 --royalty_bps 500

stellar contract invoke --id <contract-id> --source organizer --network testnet \
  -- get_event --event_id 1
```

Once deployed, hand `<contract-id>` to the backend as its
`TICKETING_CONTRACT_ID` environment variable — see the
[backend README](https://github.com/StellarTickets/backend#environment).

## Project structure

```text
.
├── contracts
│   └── ticketing
│       ├── src
│       │   ├── lib.rs             # contract logic, storage, errors, events
│       │   └── test.rs            # 30 unit tests (soroban-sdk testutils)
│       ├── test_snapshots         # recorded auth/storage snapshots per test
│       ├── Makefile
│       └── Cargo.toml
├── docs                          # architecture, API, deployment, FAQ, glossary
├── scripts
│   ├── build.sh
│   └── deploy.sh
├── .github                       # CI, issue/PR templates, dependabot
├── Cargo.toml                     # workspace manifest
├── rust-toolchain.toml            # pinned Rust version
├── rustfmt.toml / deny.toml       # formatting + dependency-audit config
└── README.md
```

## More documentation

The [`docs/`](docs/README.md) directory goes deeper on specific topics:

| Doc | Covers |
|---|---|
| [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) | How this contract fits into the wider system |
| [`CONTRACT_API.md`](docs/CONTRACT_API.md) | Full function-by-function reference |
| [`DEPLOYMENT.md`](docs/DEPLOYMENT.md) | Production deployment notes |
| [`EVENTS.md`](docs/EVENTS.md) | On-chain event schema for indexers |
| [`GAS_AND_FEES.md`](docs/GAS_AND_FEES.md) | Soroban fee model as it applies here |
| [`GLOSSARY.md`](docs/GLOSSARY.md) | Extended Soroban/Stellar terminology |
| [`INDUSTRIES.md`](docs/INDUSTRIES.md) | How the 12 supported verticals map onto `category` |
| [`TESTING.md`](docs/TESTING.md) | Test suite conventions |
| [`UPGRADES.md`](docs/UPGRADES.md) | Contract upgrade strategy |
| [`FAQ.md`](docs/FAQ.md) | Common questions |

See also [`CONTRIBUTING.md`](CONTRIBUTING.md), [`SECURITY.md`](SECURITY.md),
and [`CHANGELOG.md`](CHANGELOG.md).
