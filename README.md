# StellarTickets — Blockchain

Soroban smart contracts powering [StellarTickets](https://github.com/StellarTickets) —
*Secure. Verifiable. Powered by Stellar.*

Every ticket issued through the platform is minted as an on-chain asset on the
Stellar network via the `ticketing` contract in this repository. This is the
single source of truth for ticket ownership, validity, and resale — the
[backend](https://github.com/StellarTickets/backend) reads and writes through
this contract rather than keeping its own notion of ownership.

## Why one contract for every industry

Concerts, flights, sports, festivals, conferences, buses, movie theaters,
museums, tourist attractions, public transport, universities, and corporate
events all reduce to the same primitive: an **event** (or route, showing,
session) that issues **tickets** which must be verifiable, transferable, and
optionally resold under organizer-defined rules. Rather than one contract per
industry, `Event.category` carries the industry as metadata, and
`Event.max_resale_multiplier_bps` / `royalty_bps` let each organizer tune
anti-scalping and resale-royalty policy per event.

## Contract: `ticketing`

Path: [`contracts/ticketing`](contracts/ticketing)

| Function | Purpose |
|---|---|
| `initialize` | One-time setup: platform admin + the SEP-41 payment token used for on-chain settlement |
| `create_event` | Organizer registers an event/route/showing with resale policy |
| `issue_ticket` | Organizer-authorized mint (off-chain/fiat payment already settled) |
| `purchase_primary` | Fully on-chain primary sale — buyer pays the organizer directly, ticket mints atomically |
| `transfer_ticket` | Direct ownership transfer (gift, family member) |
| `verify_ticket` | Read-only lookup — the fraud-prevention primitive any gate scanner calls |
| `check_in` | Organizer marks a ticket used at the point of entry; cannot be replayed |
| `revoke_ticket` | Organizer voids a ticket (chargeback, counterfeit, policy violation) |
| `list_for_resale` | Owner lists a valid ticket, price capped by the event's anti-scalping multiplier |
| `cancel_resale` | Owner pulls a listing |
| `buy_resale` | Buyer purchases a listed ticket; royalty and seller proceeds settle atomically on-chain |

See [`contracts/ticketing/src/lib.rs`](contracts/ticketing/src/lib.rs) for the
full data model and [`contracts/ticketing/src/test.rs`](contracts/ticketing/src/test.rs)
for behavior specs (issuance, check-in idempotency, revocation, resale price
caps, royalty splitting).

## Development

```bash
# run the test suite
cargo test -p stellar-tickets-ticketing

# build the optimized wasm binary
stellar contract build

# deploy to testnet (requires a funded identity)
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_tickets_ticketing.wasm \
  --source <identity> \
  --network testnet
```

## Testnet setup

```bash
# create and fund a testnet identity
stellar keys generate organizer --network testnet --fund

# deploy the contract
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_tickets_ticketing.wasm \
  --source organizer \
  --network testnet

# initialize it against a testnet SEP-41 token
stellar contract invoke \
  --id <contract-id> \
  --source organizer \
  --network testnet \
  -- initialize --admin <admin-address> --payment_token <token-contract-id>
```

## Project structure

```text
.
├── contracts
│   └── ticketing
│       ├── src
│       │   ├── lib.rs      # contract logic, storage, errors, events
│       │   └── test.rs     # unit tests
│       └── Cargo.toml
├── Cargo.toml               # workspace
└── README.md
```

## More documentation

See [`docs/`](docs/README.md) for architecture, testing, deployment,
and FAQ.
