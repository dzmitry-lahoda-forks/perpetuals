// use crate::testing::setup::{self, to_decimals};
use cosmwasm_std::Uint128;
use cw20::Cw20ExecuteMsg;
use cw_multi_test::Executor;
use margined_common::integer::Integer;
use margined_perp::margined_engine::{PnlCalcOption, Side};
use margined_utils::scenarios::{to_decimals, SimpleScenario};

#[test]
fn test_partially_liquidate_long_position() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        insurance,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 25 margin * 10x position to get 20 long position
    // AMM after: 1250 : 80
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 45.18072289 margin * 1x position to get 3 short position
    // AMM after: 1204.819277 : 83
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            Uint128::from(45_180_722_890u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();

    let position = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.margin, Uint128::from(19_274_981_657u128));
    assert_eq!(position.size, Integer::new_positive(15_000_000_000u128));

    // this is todo need to add funding into the get margin ratio
    let margin_ratio = engine
        .get_margin_ratio(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(margin_ratio, Integer::new_positive(43_713_253u128));

    let carol_balance = usdc.balance(&router, carol.clone()).unwrap();
    assert_eq!(carol_balance, Uint128::from(855_695_509u128));

    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, Uint128::from(5_000_855_695_509u128));
}

#[test]
fn test_partially_liquidate_long_position_with_quote_asset_limit() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 25 margin * 10x position to get 20 long position
    // AMM after: 1250 : 80
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 45.18072289 margin * 1x position to get 3 short position
    // AMM after: 1204.819277 : 83
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            Uint128::from(45_180_722_890u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    // partially liquidate 25%
    // liquidated positionNotional: getOutputPrice(20 (original position) * 0.25) = 68.455
    // if quoteAssetAmountLimit == 273.85 > 68.455 * 4 = 273.82, quote asset gets is less than expected, thus tx reverts
    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            Uint128::from(273_850_000_000u64),
        )
        .unwrap();
    let result = router.execute(carol.clone(), msg).unwrap_err();
    assert_eq!(result.to_string(), "Generic error: reply (id 6) error \"Generic error: Less than minimum quote asset amount limit\"");

    // if quoteAssetAmountLimit == 273.8 < 68.455 * 4 = 273.82, quote asset gets is more than expected
    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            Uint128::from(273_800_000_000u64),
        )
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();
}

#[test]
fn test_partially_liquidate_short_position() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        insurance,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 20 margin * 10x position to get 25 short position
    // AMM after: 800 : 125
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 19.67213115 margin * 1x position to get 3 long position
    // AMM after: 819.6721311 : 122
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            Uint128::from(19_672_131_150u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();

    let position = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.margin, Uint128::from(16_079_605_165u128));
    assert_eq!(position.size, Integer::new_negative(18_750_000_000u128));

    let margin_ratio = engine
        .get_margin_ratio(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(margin_ratio, Integer::new_positive(45_736_327u128));

    let carol_balance = usdc.balance(&router, carol.clone()).unwrap();
    assert_eq!(carol_balance, Uint128::from(553_234_429u128));

    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, Uint128::from(5_000_553_234_429u128));
}

#[test]
fn test_partially_liquidate_short_position_with_quote_asset_limit() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 20 margin * 10x position to get 25 short position
    // AMM after: 800 : 125
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 19.67213115 margin * 1x position to get 3 long position
    // AMM after: 819.6721311 : 122
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            Uint128::from(19_672_131_150u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    // partially liquidate 25%
    // liquidated positionNotional: getOutputPrice(25 (original position) * 0.25) = 44.258
    // if quoteAssetAmountLimit == 177 > 44.258 * 4 = 177.032, quote asset pays is more than expected, thus tx reverts
    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            Uint128::from(177_000_000_000u64),
        )
        .unwrap();
    let result = router.execute(carol.clone(), msg).unwrap_err();
    assert_eq!(result.to_string(), "Generic error: reply (id 6) error \"Generic error: Greater than maximum quote asset amount limit\"");

    // if quoteAssetAmountLimit == 177.1 < 44.258 * 4 = 177.032, quote asset pays is less than expected
    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            Uint128::from(177_100_000_000u64),
        )
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();
}

#[test]
fn test_long_position_complete_liquidation() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        insurance,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 25 margin * 10x position to get 20 long position
    // AMM after: 1250 : 80
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 73.52941176 margin * 1x position to get 3 short position
    // AMM after: 1176.470588 : 85
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            Uint128::from(73_529_411_760u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let msg = engine
        .liquidate(vamm.addr().to_string(), alice.to_string(), Uint128::zero())
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();

    let position = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Integer::zero());

    let carol_balance = usdc.balance(&router, carol.clone()).unwrap();
    assert_eq!(carol_balance, Uint128::from(2_801_120_448u128));

    // 5000 - 0.91 - 2.8
    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, Uint128::from(4_996_288_515_407u128));
}

#[test]
fn test_long_position_complete_liquidation_with_slippage_limit() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 25 margin * 10x position to get 20 long position
    // AMM after: 1250 : 80
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 73.52941176 margin * 1x position to get 3 short position
    // AMM after: 1176.470588 : 85
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            Uint128::from(73_529_411_760u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            Uint128::from(224_100_000_000u128),
        )
        .unwrap();
    let result = router.execute(carol.clone(), msg).unwrap_err();
    assert_eq!(result.to_string(), "Generic error: reply (id 5) error \"Generic error: Less than minimum quote asset amount limit\"");

    let msg = engine
        .liquidate(
            vamm.addr().to_string(),
            alice.to_string(),
            to_decimals(224u64),
        )
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();
}

#[test]
fn test_short_position_complete_liquidation() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        insurance,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 20 margin * 10x position to get 25 short position
    // AMM after: 800 : 125
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when bob create a 40.33613445 margin * 1x position to get 3 long position
    // AMM after: 840.3361345 : 119
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            Uint128::from(40_336_134_450u128),
            to_decimals(1u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let msg = engine
        .liquidate(vamm.addr().to_string(), alice.to_string(), Uint128::zero())
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();

    let position = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Integer::zero());

    let carol_balance = usdc.balance(&router, carol.clone()).unwrap();
    assert_eq!(carol_balance, Uint128::from(2_793_670_659u128));

    // 5000 - 3.49 - 2.79
    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, Uint128::from(4_993_712_676_564u128));
}

#[test]
fn test_force_error_position_not_liquidation_twap_over_maintenance_margin() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when bob create a 20 margin * 5x long position when 9.0909090909 quoteAsset = 100
    // AMM after: 1100 : 90.9090909091
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(20u64),
            to_decimals(5u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // when alice create a 20 margin * 5x long position when 7.5757575758 quoteAsset = 100
    // AMM after: 1200 : 83.3333333333
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(20u64),
            to_decimals(5u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(600);
        block.height += 1;
    });

    // when bob sell his position when 7.5757575758 quoteAsset = 100
    // AMM after: 1100 : 90.9090909091
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(5u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // verify alice's openNotional = 100
    // spot price PnL = positionValue - openNotional = 84.62 - 100 = -15.38
    // TWAP PnL = (70.42 * 270 + 84.62 * 15 + 99.96 * 600 + 84.62 * 15) / 900 - 100 ~= -9.39
    // Use TWAP price PnL since -9.39 > -15.38
    let position = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.notional, to_decimals(100u64));

    let pnl = engine
        .get_unrealized_pnl(
            &router,
            vamm.addr().to_string(),
            alice.to_string(),
            PnlCalcOption::SPOTPRICE,
        )
        .unwrap();
    assert_eq!(
        pnl.unrealized_pnl,
        Integer::new_negative(15_384_615_395u128)
    );

    let pnl = engine
        .get_unrealized_pnl(
            &router,
            vamm.addr().to_string(),
            alice.to_string(),
            PnlCalcOption::TWAP,
        )
        .unwrap();
    assert_eq!(pnl.unrealized_pnl, Integer::new_negative(9_386_059_960u128));

    let price = vamm.spot_price(&router).unwrap();
    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    let msg = engine
        .liquidate(vamm.addr().to_string(), alice.to_string(), Uint128::zero())
        .unwrap();
    let result = router.execute(carol.clone(), msg).unwrap_err();
    assert_eq!(
        result.to_string(),
        "Generic error: Position is overcollateralized"
    );
}

#[test]
fn test_force_error_position_not_liquidation_spot_over_maintenance_margin() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        owner,
        engine,
        usdc,
        vamm,
        pricefeed,
        ..
    } = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = router.block_info().time.seconds();

    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    router
        .execute_contract(
            bob.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when alice create a 20 margin * 5x long position when 9.0909090909 quoteAsset = 100
    // AMM after: 1100 : 90.9090909091
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(20u64),
            to_decimals(5u64),
            to_decimals(0u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    router.update_block(|block| {
        block.time = block.time.plus_seconds(15);
        block.height += 1;
    });

    // verify alice's openNotional = 100
    // spot price PnL = positionValue - openNotional = 100 - 100 = 0
    // TWAP PnL = (83.3333333333 * 885 + 100 * 15) / 900 - 100 = -16.39
    // Use spot price PnL since 0 > -16.39
    let position = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.notional, to_decimals(100u64));

    // workaround: rounding error, should be 0 but it's actually 10 wei
    let pnl = engine
        .get_unrealized_pnl(
            &router,
            vamm.addr().to_string(),
            alice.to_string(),
            PnlCalcOption::SPOTPRICE,
        )
        .unwrap();
    assert_eq!(pnl.unrealized_pnl, Integer::new_negative(10u128));

    let pnl = engine
        .get_unrealized_pnl(
            &router,
            vamm.addr().to_string(),
            alice.to_string(),
            PnlCalcOption::TWAP,
        )
        .unwrap();
    assert_eq!(
        pnl.unrealized_pnl,
        Integer::new_negative(16_388_888_898u128)
    );

    let price = vamm.spot_price(&router).unwrap();
    let msg = pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    let msg = engine
        .liquidate(vamm.addr().to_string(), alice.to_string(), Uint128::zero())
        .unwrap();
    let result = router.execute(carol.clone(), msg).unwrap_err();
    assert_eq!(
        result.to_string(),
        "Generic error: Position is overcollateralized"
    );
}

#[test]
fn test_force_error_empty_position() {
    let SimpleScenario {
        mut router,
        alice,
        carol,
        owner,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    // set the margin ratios
    let msg = engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    let msg = engine
        .liquidate(vamm.addr().to_string(), alice.to_string(), Uint128::zero())
        .unwrap();
    let result = router.execute(carol.clone(), msg).unwrap_err();
    assert_eq!(result.to_string(), "Generic error: Position is zero");
}

#[test]
fn test_partially_liquidate_position_within_fluctuation_limit() {
    let mut env = SimpleScenario::new();

    // set the latest price
    let price: Uint128 = Uint128::from(10_000_000_000u128);
    let timestamp: u64 = env.router.block_info().time.seconds();

    let msg = env
        .pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    env.router.execute(env.owner.clone(), msg).unwrap();

    env.router.update_block(|block| {
        block.time = block.time.plus_seconds(900);
        block.height += 1;
    });

    // set the margin ratios
    let msg = env
        .engine
        .update_config(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Uint128::from(100_000_000u128)),
            Some(Uint128::from(250_000_000u128)),
            Some(Uint128::from(25_000_000u128)),
        )
        .unwrap();
    env.router.execute(env.owner.clone(), msg).unwrap();

    // reduce the allowance
    env.router
        .execute_contract(
            env.alice.clone(),
            env.usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: env.engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // reduce the allowance
    env.router
        .execute_contract(
            env.bob.clone(),
            env.usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: env.engine.addr().to_string(),
                amount: to_decimals(1900),
                expires: None,
            },
            &[],
        )
        .unwrap();

    // when bob create a 20 margin * 5x long position when 9.0909090909 quoteAsset = 100
    // AMM after: 1100 : 90.9090909091
    env.open_small_position(
        env.bob.clone(),
        Side::BUY,
        to_decimals(4u64),
        to_decimals(5u64),
        5u64,
    );

    // when alice create a 20 margin * 5x long position when 7.5757575758 quoteAsset = 100
    // AMM after: 1200 : 83.3333333333
    // alice get: 90.9090909091 - 83.3333333333 = 7.5757575758
    env.open_small_position(
        env.alice.clone(),
        Side::BUY,
        to_decimals(4u64),
        to_decimals(5u64),
        5u64,
    );

    // AMM after: 1100 : 90.9090909091, price: 12.1
    env.open_small_position(
        env.bob.clone(),
        Side::SELL,
        to_decimals(4u64),
        to_decimals(5u64),
        5u64,
    );

    let price = env.vamm.spot_price(&env.router).unwrap();
    let msg = env
        .pricefeed
        .append_price("ETH".to_string(), price, timestamp)
        .unwrap();
    env.router.execute(env.owner.clone(), msg).unwrap();

    // liquidate -> return 25% base asset to AMM
    // 90.9090909091 + 1.89 = 92.8
    // AMM after: 1077.55102 : 92.8, price: 11.61
    // fluctuation: (12.1 - 11.61116202) / 12.1 = 0.04039983306
    // values can be retrieved with amm.quoteAssetReserve() & amm.baseAssetReserve()
    let msg = env
        .engine
        .liquidate(
            env.vamm.addr().to_string(),
            env.alice.to_string(),
            to_decimals(0u64),
        )
        .unwrap();
    env.router.execute(env.carol.clone(), msg).unwrap();

    let state = env.vamm.state(&env.router).unwrap();
    assert_eq!(
        state.quote_asset_reserve,
        Uint128::from(1_077_551_020_421u128)
    );
    assert_eq!(state.base_asset_reserve, Uint128::from(92_803_030_310u128));
}
