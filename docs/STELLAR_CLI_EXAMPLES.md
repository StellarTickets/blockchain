# stellar-cli invocation examples

This document provides copy-paste examples for every function in the ticketing contract, using the `stellar` CLI.

**Prerequisites:**

- The Stellar CLI installed: [developers.stellar.org/docs/tools/developer-tools](https://developers.stellar.org/docs/tools/developer-tools)
- A testnet or mainnet contract ID (returned by `stellar contract deploy ...`)
- Testnet accounts set up and funded:

```bash
stellar keys generate organizer --network testnet --fund
stellar keys generate buyer --network testnet --fund
stellar keys generate admin --network testnet --fund
```

For examples below, substitute:
- `<contract-id>` — your deployed contract ID (starts with "C")
- `<organizer>` — the identity name (e.g., "organizer")
- `<buyer>` — another identity name (e.g., "buyer")
- `<admin>` — the admin identity (e.g., "admin")
- `<event-id>` — a u64 (e.g., 1, 12345)
- `<ticket-id>` — a u64 returned by issue_ticket or purchase_primary
- `--network testnet` — or `--network mainnet` for production

## Setup & initialization

### initialize

One-time setup. Sets the contract admin and the SEP-41 token used for primary sales and resale settlement.

```bash
# Get the native XLM token contract ID on testnet
XLM_TOKEN=$(stellar contract id asset --asset native --network testnet)

stellar contract invoke \
  --id <contract-id> \
  --source <admin> \
  --network testnet \
  -- initialize \
    --admin <admin-address> \
    --payment_token $XLM_TOKEN
```

Get your own address with:
```bash
stellar keys ls --network testnet
```

### propose_payment_token

Propose a new payment token (e.g., switching from XLM to USDC). The change is applied after `PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS` (~7 days).

```bash
USDC_TOKEN=$(stellar contract id asset --asset USDC:GBUQWP3BOUZX34ULNQG23RQ6F4YUSXHTQSXUSMIQSTBE2DFVLG7CUSG --network testnet)

stellar contract invoke \
  --id <contract-id> \
  --source <admin> \
  --network testnet \
  -- propose_payment_token \
    --admin <admin-address> \
    --new_token $USDC_TOKEN
```

### apply_payment_token

Apply a previously proposed payment token change (only works after `PAYMENT_TOKEN_CHANGE_DELAY_LEDGERS` have passed).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <admin> \
  --network testnet \
  -- apply_payment_token \
    --admin <admin-address>
```

## Event management

### create_event

Create an event. The `event_id` is chosen by the caller (typically a ULID cast to u64) so it can be correlated with the backend's database.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- create_event \
    --organizer <organizer-address> \
    --event_id 1 \
    --name '"Summer Concert 2025"' \
    --category '"concert"' \
    --max_resale_multiplier_bps 12000 \
    --royalty_bps 500
```

- `--name` and `--category` must be quoted (they're strings)
- `--max_resale_multiplier_bps 12000` = 20% markup allowed (120% of original price)
- `--royalty_bps 500` = 5% organizer royalty on resales

### create_event_with_options

Create an event with advanced options (e.g., lottery allocation or escrow).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- create_event_with_options \
    --organizer <organizer-address> \
    --event_id 2 \
    --name '"VIP Experience"' \
    --category '"vip"' \
    --max_resale_multiplier_bps 15000 \
    --royalty_bps 1000 \
    --options '{"lottery_percentage": 2500, "escrow_hold": true}'
```

### get_event

Look up an event by ID (read-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  -- get_event --event_id 1
```

### event_payment_token

Get the payment token for an event (returns the per-event override or the contract-wide default).

```bash
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  -- event_payment_token --event_id 1
```

### set_event_payment_token

Set a custom payment token for an event (organizer-only, must be done before any tickets are issued).

```bash
USDC_TOKEN=$(stellar contract id asset --asset USDC:GBUQWP3BOUZX34ULNQG23RQ6F4YUSXHTQSXUSMIQSTBE2DFVLG7CUSG --network testnet)

stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- set_event_payment_token \
    --organizer <organizer-address> \
    --event_id 1 \
    --token $USDC_TOKEN
```

## Ticket issuance

### issue_ticket

Issue a ticket (organizer-only). Use this for pre-paid or comp tickets.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- issue_ticket \
    --organizer <organizer-address> \
    --event_id 1 \
    --to <buyer-address> \
    --tier '"GA"' \
    --seat '"12A"' \
    --price 10000000
```

- `--to` is the ticket owner's address (may be different from the issuer)
- `--tier` and `--seat` are free-text strings (quoted)
- `--price` is in stroops (1 XLM = 10,000,000 stroops)

### purchase_primary

On-chain primary sale: the buyer pays the organizer in the payment token and receives the ticket atomically.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- purchase_primary \
    --buyer <buyer-address> \
    --event_id 1 \
    --tier '"VIP"' \
    --seat '"unassigned"' \
    --price 15000000
```

The buyer's account must have a balance in the payment token >= price.

## Ticket transfer & gifting

### transfer_ticket

Transfer a ticket to another owner (owner-only). Clears any active resale listing.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- transfer_ticket \
    --from <buyer-address> \
    --ticket_id <ticket-id> \
    --to <new-owner-address>
```

Fails if the ticket is used or revoked.

### transfer_batch

Transfer multiple tickets in one transaction (owner-only, max 100 per call).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- transfer_batch \
    --from <buyer-address> \
    --ticket_ids '[1, 2, 3]' \
    --to <new-owner-address>
```

### create_gift_claim

Create a gift claim link: a one-time claimable token that another user can redeem to receive a ticket.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- create_gift_claim \
    --organizer <organizer-address> \
    --event_id 1 \
    --tier '"GA"' \
    --seat '"15B"' \
    --price 0
```

Returns a `claim_id`; the organizer shares this with the recipient.

### claim_gift

Claim a gift (recipient-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- claim_gift \
    --claimer <buyer-address> \
    --claim_id <claim-id>
```

The claimer becomes the ticket owner.

## Ticket verification & check-in

### verify_ticket

Look up a ticket's current owner and status (read-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  -- verify_ticket --ticket_id <ticket-id>
```

Returns: owner address, status (Valid/Used/Revoked/Resale), original price, resale price, tier, seat, event_id.

### verify_tickets

Look up multiple tickets in one call (read-only, max 100).

```bash
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  -- verify_tickets --ticket_ids '[1, 2, 3]'
```

### check_in

Mark a ticket used at entry (organizer-only, one-way transition).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- check_in \
    --organizer <organizer-address> \
    --ticket_id <ticket-id>
```

Fails if already checked in or revoked. Emits `TicketCheckedIn` event.

### check_in_batch

Check in multiple tickets in one transaction (organizer-only, max 100 per call).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- check_in_batch \
    --organizer <organizer-address> \
    --ticket_ids '[1, 2, 3]'
```

## Ticket revocation & refunds

### revoke_ticket

Revoke a ticket (organizer-only, permanent). Prevents all future operations on it.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- revoke_ticket \
    --organizer <organizer-address> \
    --ticket_id <ticket-id>
```

### revoke_with_refund

Revoke a ticket and optionally refund the original price to the owner (organizer-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- revoke_with_refund \
    --organizer <organizer-address> \
    --ticket_id <ticket-id> \
    --refund 10000000
```

The refund amount must not exceed the original price. Payment token is automatically routed to the ticket owner.

### revoke_batch

Revoke multiple tickets in one transaction (organizer-only, max 100 per call).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- revoke_batch \
    --organizer <organizer-address> \
    --ticket_ids '[1, 2, 3]'
```

## Ticket seat management

### set_seat

Update the seat assignment on a ticket (organizer-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- set_seat \
    --organizer <organizer-address> \
    --ticket_id <ticket-id> \
    --new_seat '"13A"'
```

## Resale management

### list_for_resale

List a ticket for resale (owner-only). Price is capped at `max_resale_multiplier_bps`.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- list_for_resale \
    --owner <buyer-address> \
    --ticket_id <ticket-id> \
    --price 12000000
```

The price cannot exceed `original_price * max_resale_multiplier_bps / 10000`.

### cancel_resale

Remove a ticket from the resale list (owner-only), returning it to Valid status.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- cancel_resale \
    --owner <buyer-address> \
    --ticket_id <ticket-id>
```

### buy_resale

Buy a listed ticket (buyer-only). The organizer's royalty is paid first, then the remainder to the original seller, then ownership transfers to the buyer.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <buyer> \
  --network testnet \
  -- buy_resale \
    --buyer <buyer-address> \
    --ticket_id <ticket-id>
```

Fails if the ticket is not listed, already used, or revoked.

## Advanced: Lottery & escrow (if enabled)

### allocate_lottery

Allocate a percentage of issued tickets to a lottery pool (organizer-only, must be set during event creation).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- allocate_lottery \
    --organizer <organizer-address> \
    --event_id 1 \
    --percentage 2500
```

### enable_escrow

Enable escrow mode for an event (organizer-only), allowing ticket proceeds to be held until released.

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- enable_escrow \
    --organizer <organizer-address> \
    --event_id 1
```

### release_escrow

Release escrowed proceeds to the organizer (organizer-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <organizer> \
  --network testnet \
  -- release_escrow \
    --organizer <organizer-address> \
    --event_id 1
```

## Rate limiting

### set_purchase_throttle

Limit the number of primary sales per buyer per time window (admin-only).

```bash
stellar contract invoke \
  --id <contract-id> \
  --source <admin> \
  --network testnet \
  -- set_purchase_throttle \
    --admin <admin-address> \
    --max_per_buyer 5 \
    --window_ledgers 1000
```

## Tips for scripting

### Store contract ID in a variable

```bash
CONTRACT_ID=<contract-id>
ORGANIZER_ADDR=$(stellar keys ls --network testnet | grep organizer)
```

### Parse output

Soroban contract invocations return JSON; you can pipe to `jq`:

```bash
stellar contract invoke --id $CONTRACT_ID --network testnet \
  -- get_event --event_id 1 | jq '.organizer'
```

### Dry-run without submitting

Add `--build-only` to see the unsigned transaction (XDR) without submitting:

```bash
stellar contract invoke --id <contract-id> \
  --source <organizer> \
  --network testnet \
  --build-only \
  -- create_event ...
```

## Error codes

If a call fails, look up the error code in
[`ERRORS.md`](ERRORS.md), which lists every `Error` variant (1–34),
the entry points that return it, and whether it is worth retrying. The
enum itself is in
[`error.rs`](../contracts/ticketing/src/error.rs).

## More documentation

- [`CONTRACT_API.md`](CONTRACT_API.md) — API reference (function signatures, inputs, outputs)
- [`ERRORS.md`](ERRORS.md) — full error table and retry semantics
- [`INTEGRATION.md`](INTEGRATION.md) — backend integration guide
- [`README.md`](../README.md) — Project overview
- [`DEPLOYMENT.md`](DEPLOYMENT.md) — Testnet setup and deployment walkthrough
