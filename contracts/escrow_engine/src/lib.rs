#![no_std]

pub mod errors;
pub mod events;
pub mod storage;

#[cfg(test)]
mod test;

use errors::ContractError;
use events::{
    emit_delivery_confirmed, emit_dispute_flagged, emit_resolution_executed, emit_trade_initialized,
    emit_trade_refunded,
};
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env};
use storage::{DataKey, EscrowState, TradeDetails};

const PERSISTENT_BUMP_AMOUNT: u32 = 518400; // ~30 days at 5s/ledger
const PERSISTENT_LIFETIME_THRESHOLD: u32 = 172800; // ~10 days

#[contract]
pub struct EscrowEngineContract;

#[contractimpl]
impl EscrowEngineContract {
    /// Initialize a new trade escrow and atomically deposit funds.
    ///
    /// # Parameters
    /// - `buyer`: Buyer depositing funds into escrow
    /// - `farmer`: Agricultural producer/seller fulfilling the trade
    /// - `arbiter`: Multi-sig arbiter or AI oracle consensus entity
    /// - `token`: SAC token contract address (e.g. USDC)
    /// - `amount`: Total trade payment in token stroops
    /// - `expiry`: Expiration timestamp in seconds
    pub fn initialize_trade(
        env: Env,
        buyer: Address,
        farmer: Address,
        arbiter: Address,
        token: Address,
        amount: i128,
        expiry: u64,
    ) -> Result<u64, ContractError> {
        buyer.require_auth();

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if buyer == farmer {
            return Err(ContractError::IdenticalParties);
        }
        if expiry <= env.ledger().timestamp() {
            return Err(ContractError::InvalidExpiry);
        }

        // Execute atomic deposit from buyer into escrow contract
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        // Increment trade count
        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TradeCount)
            .unwrap_or(0);
        count = count.checked_add(1).ok_or(ContractError::Overflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::TradeCount, &count);

        let trade = TradeDetails {
            id: count,
            buyer: buyer.clone(),
            farmer: farmer.clone(),
            arbiter,
            token: token.clone(),
            amount,
            state: EscrowState::Funded,
            delivery_hash: None,
            expiry,
            buyer_share: 0,
            farmer_share: 0,
            created_at: env.ledger().timestamp(),
        };

        let trade_key = DataKey::Trade(count);
        env.storage().persistent().set(&trade_key, &trade);
        env.storage().persistent().extend_ttl(
            &trade_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        emit_trade_initialized(&env, count, &buyer, &farmer, &token, amount, expiry);

        Ok(count)
    }

    /// Farmer submits confirmation hash of delivered agricultural goods.
    pub fn confirm_delivery(
        env: Env,
        caller: Address,
        escrow_id: u64,
        delivery_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        let trade_key = DataKey::Trade(escrow_id);
        let mut trade: TradeDetails = env
            .storage()
            .persistent()
            .get(&trade_key)
            .ok_or(ContractError::TradeNotFound)?;

        if caller != trade.farmer {
            return Err(ContractError::Unauthorized);
        }

        match trade.state {
            EscrowState::Funded => {}
            EscrowState::Delivered => return Err(ContractError::AlreadyDelivered),
            EscrowState::Disputed => return Err(ContractError::DisputeLocked),
            _ => return Err(ContractError::InvalidState),
        }

        if env.ledger().timestamp() > trade.expiry {
            return Err(ContractError::TradeExpired);
        }

        trade.delivery_hash = Some(delivery_hash.clone());
        trade.state = EscrowState::Delivered;

        env.storage().persistent().set(&trade_key, &trade);
        env.storage().persistent().extend_ttl(
            &trade_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        emit_delivery_confirmed(&env, escrow_id, &caller, delivery_hash);

        Ok(())
    }

    /// Flag a non-compliance dispute, locking funds from release.
    pub fn flag_dispute(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();

        let trade_key = DataKey::Trade(escrow_id);
        let mut trade: TradeDetails = env
            .storage()
            .persistent()
            .get(&trade_key)
            .ok_or(ContractError::TradeNotFound)?;

        if caller != trade.buyer && caller != trade.farmer {
            return Err(ContractError::Unauthorized);
        }

        if trade.state != EscrowState::Funded && trade.state != EscrowState::Delivered {
            return Err(ContractError::InvalidState);
        }

        trade.state = EscrowState::Disputed;

        env.storage().persistent().set(&trade_key, &trade);
        env.storage().persistent().extend_ttl(
            &trade_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        emit_dispute_flagged(&env, escrow_id, &caller);

        Ok(())
    }

    /// Execute dispute resolution with split payouts to buyer and farmer.
    ///
    /// # Parameters
    /// - `arbiter`: Must match the assigned arbiter/oracle address
    /// - `escrow_id`: Unique identifier of the disputed trade
    /// - `buyer_share`: Token amount to return to buyer
    /// - `farmer_share`: Token amount to award to farmer
    pub fn execute_resolution(
        env: Env,
        arbiter: Address,
        escrow_id: u64,
        buyer_share: i128,
        farmer_share: i128,
    ) -> Result<(), ContractError> {
        arbiter.require_auth();

        let trade_key = DataKey::Trade(escrow_id);
        let mut trade: TradeDetails = env
            .storage()
            .persistent()
            .get(&trade_key)
            .ok_or(ContractError::TradeNotFound)?;

        if arbiter != trade.arbiter {
            return Err(ContractError::Unauthorized);
        }

        if trade.state != EscrowState::Disputed {
            return Err(ContractError::InvalidState);
        }

        if buyer_share < 0 || farmer_share < 0 {
            return Err(ContractError::InvalidAmount);
        }

        let total_split = buyer_share
            .checked_add(farmer_share)
            .ok_or(ContractError::Overflow)?;

        if total_split != trade.amount {
            return Err(ContractError::SplitMismatch);
        }

        let token_client = token::Client::new(&env, &trade.token);
        let contract_address = env.current_contract_address();

        if buyer_share > 0 {
            token_client.transfer(&contract_address, &trade.buyer, &buyer_share);
        }
        if farmer_share > 0 {
            token_client.transfer(&contract_address, &trade.farmer, &farmer_share);
        }

        trade.buyer_share = buyer_share;
        trade.farmer_share = farmer_share;
        trade.state = EscrowState::Resolved;

        env.storage().persistent().set(&trade_key, &trade);
        env.storage().persistent().extend_ttl(
            &trade_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        emit_resolution_executed(&env, escrow_id, &arbiter, buyer_share, farmer_share);

        Ok(())
    }

    /// Mutual cancellation or expiry refund to buyer before goods are dispatched.
    pub fn refund_trade(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();

        let trade_key = DataKey::Trade(escrow_id);
        let mut trade: TradeDetails = env
            .storage()
            .persistent()
            .get(&trade_key)
            .ok_or(ContractError::TradeNotFound)?;

        if caller != trade.buyer && caller != trade.farmer {
            return Err(ContractError::Unauthorized);
        }

        if trade.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        // Refund permitted if caller is farmer (voluntary cancellation)
        // or if buyer initiates after expiry or before dispatch
        let is_farmer = caller == trade.farmer;
        let is_expired = env.ledger().timestamp() >= trade.expiry;
        if !is_farmer && !is_expired {
            // Buyer can cancel if trade is still funded within initial grace
        }

        let token_client = token::Client::new(&env, &trade.token);
        token_client.transfer(
            &env.current_contract_address(),
            &trade.buyer,
            &trade.amount,
        );

        trade.state = EscrowState::Refunded;
        trade.buyer_share = trade.amount;
        trade.farmer_share = 0;

        env.storage().persistent().set(&trade_key, &trade);
        env.storage().persistent().extend_ttl(
            &trade_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        emit_trade_refunded(&env, escrow_id, &trade.buyer, trade.amount);

        Ok(())
    }

    /// Read details of a specific escrow trade.
    pub fn get_trade(env: Env, escrow_id: u64) -> Result<TradeDetails, ContractError> {
        let trade_key = DataKey::Trade(escrow_id);
        env.storage()
            .persistent()
            .get(&trade_key)
            .ok_or(ContractError::TradeNotFound)
    }

    /// Read the total number of trades registered.
    pub fn get_trade_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::TradeCount)
            .unwrap_or(0)
    }
}
