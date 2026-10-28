#![cfg(test)]

use crate::{
    errors::ContractError,
    storage::EscrowState,
    EscrowEngineContract, EscrowEngineContractClient,
};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env,
};

struct TestFixture<'a> {
    env: Env,
    client: EscrowEngineContractClient<'a>,
    token_client: TokenClient<'a>,
    token_admin_client: StellarAssetClient<'a>,
    token_address: Address,
    buyer: Address,
    farmer: Address,
    arbiter: Address,
}

impl<'a> TestFixture<'a> {
    fn setup() -> Self {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let buyer = Address::generate(&env);
        let farmer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin);
        let token_address = sac.address();

        let contract_id = env.register(EscrowEngineContract, ());
        let client = EscrowEngineContractClient::new(&env, &contract_id);

        let token_client = TokenClient::new(&env, &token_address);
        let token_admin_client = StellarAssetClient::new(&env, &token_address);

        Self {
            env,
            client,
            token_client,
            token_admin_client,
            token_address,
            buyer,
            farmer,
            arbiter,
        }
    }
}

#[test]
fn test_initialize_trade_happy_path() {
    let fix = TestFixture::setup();
    let amount = 10_000_000i128; // 10 USDC
    fix.token_admin_client.mint(&fix.buyer, &amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &amount,
        &expiry,
    );

    assert_eq!(trade_id, 1);
    assert_eq!(fix.client.get_trade_count(), 1);

    let trade = fix.client.get_trade(&trade_id);
    assert_eq!(trade.id, 1);
    assert_eq!(trade.buyer, fix.buyer);
    assert_eq!(trade.farmer, fix.farmer);
    assert_eq!(trade.arbiter, fix.arbiter);
    assert_eq!(trade.amount, amount);
    assert_eq!(trade.state, EscrowState::Funded);
    assert_eq!(trade.delivery_hash, None);

    // Contract now holds deposited funds
    assert_eq!(
        fix.token_client
            .balance(&fix.client.address),
        amount
    );
    assert_eq!(fix.token_client.balance(&fix.buyer), 0);
}

#[test]
fn test_confirm_delivery_happy_path() {
    let fix = TestFixture::setup();
    let amount = 5_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &amount,
        &expiry,
    );

    let delivery_hash = BytesN::from_array(&fix.env, &[42u8; 32]);
    fix.client
        .confirm_delivery(&fix.farmer, &trade_id, &delivery_hash);

    let trade = fix.client.get_trade(&trade_id);
    assert_eq!(trade.state, EscrowState::Delivered);
    assert_eq!(trade.delivery_hash, Some(delivery_hash));
}

#[test]
fn test_confirm_delivery_unauthorized_rejection() {
    let fix = TestFixture::setup();
    let amount = 5_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &amount,
        &expiry,
    );

    let delivery_hash = BytesN::from_array(&fix.env, &[42u8; 32]);
    // Buyer or stranger cannot confirm delivery on behalf of farmer
    let stranger = Address::generate(&fix.env);
    let res = fix
        .client
        .try_confirm_delivery(&stranger, &trade_id, &delivery_hash);
    assert_eq!(res, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn test_flag_dispute_by_buyer_and_farmer() {
    let fix = TestFixture::setup();
    let amount = 20_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &amount,
        &expiry,
    );

    // Buyer flags dispute
    fix.client.flag_dispute(&fix.buyer, &trade_id);

    let trade = fix.client.get_trade(&trade_id);
    assert_eq!(trade.state, EscrowState::Disputed);

    // Delivery confirmation is locked once disputed
    let delivery_hash = BytesN::from_array(&fix.env, &[1u8; 32]);
    let err = fix
        .client
        .try_confirm_delivery(&fix.farmer, &trade_id, &delivery_hash);
    assert_eq!(err, Err(Ok(ContractError::DisputeLocked)));
}

#[test]
fn test_flag_dispute_unauthorized() {
    let fix = TestFixture::setup();
    let amount = 10_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &amount,
        &expiry,
    );

    let unauthorized = Address::generate(&fix.env);
    let res = fix.client.try_flag_dispute(&unauthorized, &trade_id);
    assert_eq!(res, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn test_execute_resolution_happy_path_split() {
    let fix = TestFixture::setup();
    let total_amount = 10_000_000i128; // 10 USDC
    fix.token_admin_client.mint(&fix.buyer, &total_amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &total_amount,
        &expiry,
    );

    // Flag dispute
    fix.client.flag_dispute(&fix.buyer, &trade_id);

    // 70% refund to buyer, 30% payout to farmer based on AI evidence evaluation
    let buyer_share = 7_000_000i128;
    let farmer_share = 3_000_000i128;

    fix.client.execute_resolution(
        &fix.arbiter,
        &trade_id,
        &buyer_share,
        &farmer_share,
    );

    let trade = fix.client.get_trade(&trade_id);
    assert_eq!(trade.state, EscrowState::Resolved);
    assert_eq!(trade.buyer_share, buyer_share);
    assert_eq!(trade.farmer_share, farmer_share);

    // Balances verified atomically
    assert_eq!(fix.token_client.balance(&fix.buyer), buyer_share);
    assert_eq!(fix.token_client.balance(&fix.farmer), farmer_share);
    assert_eq!(
        fix.token_client
            .balance(&fix.client.address),
        0
    );
}

#[test]
fn test_execute_resolution_split_mismatch_rejection() {
    let fix = TestFixture::setup();
    let total_amount = 10_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &total_amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &total_amount,
        &expiry,
    );

    fix.client.flag_dispute(&fix.farmer, &trade_id);

    // Split does not equal total amount (6M + 3M != 10M)
    let res = fix.client.try_execute_resolution(
        &fix.arbiter,
        &trade_id,
        &6_000_000,
        &3_000_000,
    );
    assert_eq!(res, Err(Ok(ContractError::SplitMismatch)));
}

#[test]
fn test_execute_resolution_unauthorized_arbiter() {
    let fix = TestFixture::setup();
    let total_amount = 10_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &total_amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &total_amount,
        &expiry,
    );

    fix.client.flag_dispute(&fix.buyer, &trade_id);

    let fake_arbiter = Address::generate(&fix.env);
    let res = fix.client.try_execute_resolution(
        &fake_arbiter,
        &trade_id,
        &5_000_000,
        &5_000_000,
    );
    assert_eq!(res, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn test_execute_resolution_undisputed_rejected() {
    let fix = TestFixture::setup();
    let total_amount = 10_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &total_amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &total_amount,
        &expiry,
    );

    // Attempting resolution while still Funded (not Disputed)
    let res = fix.client.try_execute_resolution(
        &fix.arbiter,
        &trade_id,
        &5_000_000,
        &5_000_000,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn test_refund_trade_happy_path() {
    let fix = TestFixture::setup();
    let total_amount = 15_000_000i128;
    fix.token_admin_client.mint(&fix.buyer, &total_amount);

    let expiry = fix.env.ledger().timestamp() + 3600;
    let trade_id = fix.client.initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &total_amount,
        &expiry,
    );

    // Buyer refunds before dispatch
    fix.client.refund_trade(&fix.buyer, &trade_id);

    let trade = fix.client.get_trade(&trade_id);
    assert_eq!(trade.state, EscrowState::Refunded);
    assert_eq!(trade.buyer_share, total_amount);
    assert_eq!(fix.token_client.balance(&fix.buyer), total_amount);
    assert_eq!(
        fix.token_client
            .balance(&fix.client.address),
        0
    );
}

#[test]
fn test_validation_errors() {
    let fix = TestFixture::setup();
    let expiry = fix.env.ledger().timestamp() + 3600;

    // Zero amount rejected
    let res = fix.client.try_initialize_trade(
        &fix.buyer,
        &fix.farmer,
        &fix.arbiter,
        &fix.token_address,
        &0,
        &expiry,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidAmount)));

    // Identical buyer and farmer rejected
    let res2 = fix.client.try_initialize_trade(
        &fix.buyer,
        &fix.buyer,
        &fix.arbiter,
        &fix.token_address,
        &1_000_000,
        &expiry,
    );
    assert_eq!(res2, Err(Ok(ContractError::IdenticalParties)));
}
