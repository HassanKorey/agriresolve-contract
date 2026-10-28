use soroban_sdk::{symbol_short, Address, BytesN, Env};

pub fn emit_trade_initialized(
    env: &Env,
    trade_id: u64,
    buyer: &Address,
    farmer: &Address,
    token: &Address,
    amount: i128,
    expiry: u64,
) {
    let topics = (symbol_short!("init"), trade_id, buyer.clone());
    env.events()
        .publish(topics, (farmer.clone(), token.clone(), amount, expiry));
}

pub fn emit_delivery_confirmed(
    env: &Env,
    trade_id: u64,
    farmer: &Address,
    delivery_hash: BytesN<32>,
) {
    let topics = (symbol_short!("deliver"), trade_id, farmer.clone());
    env.events().publish(topics, delivery_hash);
}

pub fn emit_dispute_flagged(env: &Env, trade_id: u64, caller: &Address) {
    let topics = (symbol_short!("dispute"), trade_id, caller.clone());
    env.events().publish(topics, ());
}

pub fn emit_resolution_executed(
    env: &Env,
    trade_id: u64,
    arbiter: &Address,
    buyer_share: i128,
    farmer_share: i128,
) {
    let topics = (symbol_short!("resolve"), trade_id, arbiter.clone());
    env.events().publish(topics, (buyer_share, farmer_share));
}

pub fn emit_trade_refunded(env: &Env, trade_id: u64, buyer: &Address, amount: i128) {
    let topics = (symbol_short!("refund"), trade_id, buyer.clone());
    env.events().publish(topics, amount);
}
