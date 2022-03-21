use cosmwasm_std::Uint128;
use cw_multi_test::Executor;
use margined_common::integer::Integer;
use margined_perp::margined_engine::Side;
use margined_utils::scenarios::SimpleScenario;

pub const DECIMAL_MULTIPLIER: Uint128 = Uint128::new(1_000_000_000);

// takes in a Uint128 and multiplies by the decimals just to make tests more legible
pub fn to_decimals(input: u64) -> Uint128 {
    Uint128::from(input) * DECIMAL_MULTIPLIER
}

#[test]
fn test_generate_loss_for_amm_when_funding_rate_is_positive_and_amm_is_long() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        owner,
        insurance,
        engine,
        vamm,
        usdc,
        pricefeed,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(300u64),
            to_decimals(2u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(1200u64),
            to_decimals(1u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, Uint128::from(1_500_000_000_000u128));

    let price: Uint128 = Uint128::from(1_590_000_000u128);
    let timestamp: u64 = 1_000_000_000;

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // move to the next funding time
    router.update_block(|block| {
        block.time = block.time.plus_seconds(86_400u64);
        block.height += 1;
    });

    let msg = engine.pay_funding(vamm.addr().to_string()).unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // then alice need to pay 1% of her position size as fundingPayment
    // {balance: 37.5, margin: 300} => {balance: 37.5, margin: 299.625}
    let alice_position = engine
        .get_position_with_funding_payment(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(
        alice_position.size,
        Integer::new_positive(37_500_000_000u128)
    );
    assert_eq!(alice_position.margin, Uint128::from(299_625_000_000u128));

    // then bob will get 1% of his position size as fundingPayment
    // {balance: -187.5, margin: 1200} => {balance: -187.5, margin: 1201.875}
    let bob_position = engine
        .get_position_with_funding_payment(&router, vamm.addr().to_string(), bob.to_string())
        .unwrap();
    assert_eq!(
        bob_position.size,
        Integer::new_negative(187_500_000_000u128)
    );
    assert_eq!(bob_position.margin, Uint128::from(1_201_875_000_000u128));

    // then fundingPayment will generate 1.5 loss and clearingHouse will withdraw in advanced from insuranceFund
    // clearingHouse: 1500 + 1.5
    // insuranceFund: 5000 - 1.5
    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, Uint128::from(1_501_500_000_000u128));
    let insurance_balance = usdc.balance(&router, insurance).unwrap();
    assert_eq!(insurance_balance, Uint128::from(4_998_500_000_000u128));
}

#[test]
fn test_will_keep_generating_same_loss_when_funding_rate_is_positive() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        owner,
        insurance,
        engine,
        vamm,
        usdc,
        pricefeed,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(300u64),
            to_decimals(2u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(1200u64),
            to_decimals(1u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let price: Uint128 = Uint128::from(1_590_000_000u128);
    let timestamp: u64 = 1_000_000_000;

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // move to the next funding time
    router.update_block(|block| {
        block.time = block.time.plus_seconds(86_400u64);
        block.height += 1;
    });

    let msg = engine.pay_funding(vamm.addr().to_string()).unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // move to the next funding time
    router.update_block(|block| {
        block.time = block.time.plus_seconds(86_400u64);
        block.height += 1;
    });

    let msg = engine.pay_funding(vamm.addr().to_string()).unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // same as above test case:
    // there are only 2 traders: bob and alice
    // alice need to pay 1% of her position size as fundingPayment (37.5 * 1% = 0.375)
    // bob will get 1% of her position size as fundingPayment (187.5 * 1% = 1.875)
    // ammPnl = 0.375 - 1.875 = -1.5
    // clearingHouse payFunding twice in the same condition
    // then fundingPayment will generate 1.5 * 2 loss and clearingHouse will withdraw in advanced from insuranceFund
    // clearingHouse: 1500 + 3
    // insuranceFund: 5000 - 3
    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, Uint128::from(1_503_000_000_000u128));
    let insurance_balance = usdc.balance(&router, insurance).unwrap();
    assert_eq!(insurance_balance, Uint128::from(4_997_000_000_000u128));
}
