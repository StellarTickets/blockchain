# On-chain events

The contract publishes Soroban events, defined with
`#[contractevent]` in `lib.rs`:

Failed calls do not publish failure events. Soroban rolls back the complete
failed transaction, including its events; see [`ERRORS.md`](ERRORS.md) for
the structured-error decision and recommended off-chain context.

## `TicketIssued`

Emitted by `issue_ticket` and `purchase_primary`.

| Field       | Type      | Notes            |
| ----------- | --------- | ---------------- |
| `ticket_id` | `u64`     | topic            |
| `event_id`  | `u64`     |                  |
| `owner`     | `Address` | Ticket owner     |
| `tier`      | `String`  | Ticket tier name |

## `TicketCheckedIn`

Emitted by `check_in`.

| Field       | Type      | Notes |
| ----------- | --------- | ----- |
| `ticket_id` | `u64`     | topic |
| `organizer` | `Address` |       |

Indexers and the backend's reconciliation job can subscribe to these
instead of polling `get_ticket` for every ticket on every block.
[`examples/indexer`](../examples/indexer/README.md) is a sample that decodes
every event below using the contract spec attached to each release.

## `TicketTransferred`

Emitted by `transfer_ticket` after the ownership change is saved (issue #115).

| Field       | Type      | Notes                     |
| ----------- | --------- | ------------------------- |
| `ticket_id` | `u64`     | topic                     |
| `from`      | `Address` | Owner before the transfer |
| `to`        | `Address` | Owner after the transfer  |

Every owner change publishes this event (issue #513), whichever entry point
made it: `transfer_ticket`, `transfer_from`, `transfer_batch` (one event per
ticket), `claim_gift`, `buy_resale`, `settle_auction` (to the winner) and
`accept_offer`. The event is published alongside the entry point's own event,
such as `TicketResold`, `AuctionSettled` or `OfferAccepted`. A rejected call
publishes nothing. Indexers can therefore follow this one event for every owner
change, without polling `owner_of`.

## `EventCreated`

Emitted by `create_event` and `create_event_with_options` after the event is
saved (issue #119).

| Field       | Type      | Notes                                      |
| ----------- | --------- | ------------------------------------------ |
| `organizer` | `Address` | topic. Organizer that registered the event |
| `event_id`  | `u64`     | topic                                      |

## `PrimarySale`

Emitted by `purchase_primary` after the ticket is minted (issue #118).

| Field       | Type      | Notes                                     |
| ----------- | --------- | ----------------------------------------- |
| `ticket_id` | `u64`     | topic                                     |
| `buyer`     | `Address` | Address that paid for the ticket          |
| `price`     | `i128`    | Amount paid, in the event's payment token |

## `TicketListed`

Emitted by `list_for_resale` after the listing is saved (issue #116).

| Field       | Type      | Notes                                        |
| ----------- | --------- | -------------------------------------------- |
| `ticket_id` | `u64`     | topic                                        |
| `owner`     | `Address` | Seller who listed the ticket                 |
| `price`     | `i128`    | Listing price, within the event's resale cap |

## `ListingCancelled`

Emitted by `cancel_resale` after the listing is removed (issue #116).

| Field       | Type      | Notes                           |
| ----------- | --------- | ------------------------------- |
| `ticket_id` | `u64`     | topic                           |
| `owner`     | `Address` | Owner who cancelled the listing |

## `TicketResold`

Emitted by `buy_resale` after ownership and the royalty split are settled
(issue #117).

| Field       | Type      | Notes                                                     |
| ----------- | --------- | --------------------------------------------------------- |
| `ticket_id` | `u64`     | topic                                                     |
| `seller`    | `Address` | Owner before the purchase                                 |
| `buyer`     | `Address` | Owner after the purchase                                  |
| `price`     | `i128`    | Full resale price paid by the buyer                       |
| `royalty`   | `i128`    | Part of `price` paid to the organizer (or royalty splits) |

Resale purchases also publish `TicketResold`, so marketplace indexers do not
need to derive the price or royalty from token transfers.

## `AuctionStarted`

Emitted when an auction starts on a ticket (issue #105).

| Field           | Type      | Notes                                |
| --------------- | --------- | ------------------------------------ |
| `ticket_id`     | `u64`     | topic                                |
| `seller`        | `Address` | Owner who started the auction        |
| `reserve_price` | `i128`    | Minimum accepted bid                 |
| `ends_at`       | `u64`     | Ledger timestamp when bidding closes |

## `BidPlaced`

Emitted when a bid is placed on an auctioned ticket (issue #105).

| Field       | Type      | Notes                   |
| ----------- | --------- | ----------------------- |
| `ticket_id` | `u64`     | topic                   |
| `bidder`    | `Address` | Address placing the bid |
| `amount`    | `i128`    | Bid amount              |

## `AuctionSettled`

Emitted when an auction settles and ownership transfers to the winner (issue #105).

| Field       | Type      | Notes              |
| ----------- | --------- | ------------------ |
| `ticket_id` | `u64`     | topic              |
| `buyer`     | `Address` | Winning bidder     |
| `amount`    | `i128`    | Winning bid amount |

## `OfferMade`

Emitted when a buyer makes an offer on a listed ticket (issue #106).

| Field       | Type      | Notes                    |
| ----------- | --------- | ------------------------ |
| `ticket_id` | `u64`     | topic                    |
| `buyer`     | `Address` | Address making the offer |
| `amount`    | `i128`    | Offer amount             |

## `OfferAccepted`

Emitted when a seller accepts an offer (issue #106).

| Field       | Type      | Notes                            |
| ----------- | --------- | -------------------------------- |
| `ticket_id` | `u64`     | topic                            |
| `buyer`     | `Address` | Address whose offer was accepted |
| `amount`    | `i128`    | Accepted offer amount            |

## `OfferCancelled`

Emitted when a buyer cancels their offer (issue #106).

| Field       | Type      | Notes                        |
| ----------- | --------- | ---------------------------- |
| `ticket_id` | `u64`     | topic                        |
| `buyer`     | `Address` | Address cancelling the offer |
| `amount`    | `i128`    | Cancelled offer amount       |

## Admin events

| Event                     | Emitted by                                 | Fields                                                            |
| ------------------------- | ------------------------------------------ | ----------------------------------------------------------------- |
| `ContractInitialized`     | `initialize`                               | `admin` (topic), `payment_token`                                  |
| `PurchaseThrottleUpdated` | `set_purchase_throttle`                    | `admin` (topic), `min_ledger_spacing`                             |
| `OrganizerApproved`       | `approve_organizer`                        | `admin` (topic), `organizer`                                      |
| `OrganizerRevoked`        | `revoke_organizer`                         | `admin` (topic), `organizer`                                      |
| `PaymentTokenProposed`    | `propose_payment_token`                    | `admin` (topic), `new_token`, `apply_after_ledger`                |
| `PaymentTokenChanged`     | `apply_payment_token`                      | `admin` (topic), `old_token`, `new_token`                         |
| `ContractUpgraded`        | `upgrade`                                  | `admin` (topic), `new_wasm_hash` (see [UPGRADES.md](UPGRADES.md)) |
| `AdminProposed`           | `propose_admin`                            | `admin` (topic), `new_admin`                                      |
| `AdminTransferred`        | `accept_admin`                             | `old_admin` (topic), `new_admin`                                  |
| `PlatformFeeUpdated`      | `set_platform_fee_bps`, `set_platform_fee` | `admin` (topic), `fee_bps`, `recipient`                           |
