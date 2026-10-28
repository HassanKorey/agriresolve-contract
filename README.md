# AgriResolve: Escrow Engine Contract (`agriresolve-contract`)

> Cryptographically secure Soroban agricultural dispute escrow state machine on Stellar.

## Overview

AgriResolve protects agricultural trade participants in emerging markets by securing transaction value in a non-custodial Soroban escrow state machine. Disputes flagged off-chain are investigated by AI provenance auditors and cryptographically resolved through on-chain multi-sig/oracle settlements.

## State Machine (`EscrowState`)

```
   [Initiate Trade]
          │
          ▼
     ┌─────────┐
     │ Funded  │ ◄────── Buyer deposits tokens
     └────┬────┘
          │
    ┌─────┴──────────────────┐
    ▼                        ▼
┌───────────┐          ┌──────────┐
│ Delivered │          │ Refunded │ ◄── Buyer refunded if canceled before dispatch
└─────┬─────┘          └──────────┘
      │
      ├──────────────────────┐
      ▼                      ▼
┌───────────┐          ┌──────────┐
│ Disputed  │          │ Resolved │
└─────┬─────┘          └──────────┘
      │
      ▼
┌───────────┐
│ Resolved  │ ◄── Arbitrator/AI consensus atomic split payout
└───────────┘
```

## Functions

- `initialize_trade(buyer, farmer, arbiter, token, amount, expiry)`: Buyer deposits collateral into escrow.
- `confirm_delivery(caller, escrow_id, delivery_hash)`: Farmer submits delivery proof hash.
- `flag_dispute(caller, escrow_id)`: Buyer or farmer flags non-compliance, locking funds.
- `execute_resolution(arbiter, escrow_id, buyer_share, farmer_share)`: Authorized arbiter / oracle multisig releases verified split payout atomically via SAC token client.
- `refund_trade(caller, escrow_id)`: Mutual or pre-dispatch return of funds to buyer.
- `get_trade(escrow_id)`: Read current trade details and status.
- `get_trade_count()`: Read total registered trades.

## Build & Test

```bash
# Run unit tests
cargo test

# Build optimized WebAssembly contract
cargo build --target wasm32-unknown-unknown --release
```
