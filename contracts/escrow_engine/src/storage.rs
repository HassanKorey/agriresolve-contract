use soroban_sdk::{contracttype, Address, BytesN};

/// Lifecycle states of an agricultural trade escrow.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum EscrowState {
    /// Buyer has funded USDC/token into the escrow.
    Funded = 0,
    /// Farmer has delivered goods and submitted on-chain delivery confirmation hash.
    Delivered = 1,
    /// Buyer or farmer flagged non-compliance or discrepancy, locking funds.
    Disputed = 2,
    /// Arbitrator / AI oracle consensus has finalized split payout resolution.
    Resolved = 3,
    /// Mutual cancellation or expiry before dispatch; buyer fully refunded.
    Refunded = 4,
}

/// Comprehensive on-chain trade record.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TradeDetails {
    pub id: u64,
    pub buyer: Address,
    pub farmer: Address,
    pub arbiter: Address,
    pub token: Address,
    pub amount: i128,
    pub state: EscrowState,
    pub delivery_hash: Option<BytesN<32>>,
    pub expiry: u64,
    pub buyer_share: i128,
    pub farmer_share: i128,
    pub created_at: u64,
}

/// Persistent storage keys for the Escrow Engine contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    TradeCount,
    Trade(u64),
}
