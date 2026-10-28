use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    TradeNotFound = 4,
    InvalidState = 5,
    InvalidAmount = 6,
    TradeExpired = 7,
    SplitMismatch = 8,
    Overflow = 9,
    AlreadyDelivered = 10,
    DisputeLocked = 11,
    ZeroAddress = 12,
    IdenticalParties = 13,
    InvalidExpiry = 14,
}
