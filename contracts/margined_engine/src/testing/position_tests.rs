// use crate::testing::setup::{self, to_decimals};
use cosmwasm_std::Uint128;
use cw20::Cw20ExecuteMsg;
use cw_multi_test::Executor;
use margined_perp::margined_engine::{PositionResponse, Side};
use margined_utils::scenarios::SimpleScenario;

pub const DECIMAL_MULTIPLIER: Uint128 = Uint128::new(1_000_000_000);

// takes in a Uint128 and multiplies by the decimals just to make tests more legible
pub fn to_decimals(input: u64) -> Uint128 {
    Uint128::from(input) * DECIMAL_MULTIPLIER
}

#[test]
fn test_initialization() {
    let SimpleScenario {
        router,
        owner,
        alice,
        bob,
        usdc,
        engine,
        ..
    } = SimpleScenario::new();

    // verfiy the balances
    let owner_balance = usdc.balance(&router, owner.clone()).unwrap();
    assert_eq!(owner_balance, Uint128::zero());
    let alice_balance = usdc.balance(&router, alice.clone()).unwrap();
    assert_eq!(alice_balance, Uint128::new(5_000_000_000_000));
    let bob_balance = usdc.balance(&router, bob.clone()).unwrap();
    assert_eq!(bob_balance, Uint128::new(5_000_000_000_000));
    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, Uint128::zero());
}

#[test]
fn test_open_position_long() {
    let SimpleScenario {
        mut router,
        alice,
        usdc,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // expect to be 60
    let margin = engine.trader_balance(&router, alice.to_string()).unwrap();
    assert_eq!(margin, to_decimals(60));

    // personal position should be 37.5
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::new(37_500_000_000));
    assert_eq!(position.margin, to_decimals(60u64));

    // clearing house token balance should be 60
    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, to_decimals(60));
}

#[test]
fn test_open_position_two_longs() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // expect to be 120
    let margin = engine.trader_balance(&router, alice.to_string()).unwrap();
    assert_eq!(margin, to_decimals(120));

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::new(54_545_454_545));
    assert_eq!(position.margin, to_decimals(120));
}

#[test]
fn test_open_position_two_shorts() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(40u64),
            to_decimals(5u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(40u64),
            to_decimals(5u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // personal balance with funding payment
    let margin = engine.trader_balance(&router, alice.to_string()).unwrap();
    assert_eq!(margin, to_decimals(80));

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::new(66_666_666_667));
    assert_eq!(position.margin, to_decimals(80));
}

#[test]
fn test_open_position_equal_size_opposite_side() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(300u64),
            to_decimals(2u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // personal balance with funding payment
    let margin = engine.trader_balance(&router, alice.to_string()).unwrap();
    assert_eq!(margin, Uint128::zero());

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());
    assert_eq!(position.margin, Uint128::zero());
}

#[test]
fn test_open_position_one_long_two_shorts() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(5u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::new(33_333_333_333));
    assert_eq!(position.margin, to_decimals(60));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(50u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // personal balance with funding payment
    let margin = engine.trader_balance(&router, alice.to_string()).unwrap();
    assert_eq!(margin, Uint128::zero());

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());
    assert_eq!(position.margin, Uint128::zero());
}

#[test]
fn test_open_position_short_and_two_longs() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(40u64),
            to_decimals(5u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::new(25_000_000_000));
    assert_eq!(position.margin, to_decimals(40));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(20u64),
            to_decimals(5u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::new(11_111_111_112));
    assert_eq!(position.margin, to_decimals(40));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(10u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::from(1_u128));
    assert_eq!(position.margin, to_decimals(40u64));
}

#[test]
fn test_open_position_short_long_short() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(150u64),
            to_decimals(3u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, to_decimals(20u64));
    assert_eq!(position.margin, Uint128::new(83_333_333_333));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(25u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());
    assert_eq!(position.margin, Uint128::zero());
}

#[test]
fn test_open_position_long_short_long() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(150u64),
            to_decimals(3u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, to_decimals(25u64));
    assert_eq!(position.margin, Uint128::new(66_666_666_666));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(20u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());
    assert_eq!(position.margin, Uint128::zero());
}

#[test]
fn test_pnl_zero_no_others_trading() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(250u64),
            to_decimals(1u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(750u64),
            to_decimals(1u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let pnl: Uint128 = engine
        .unrealized_pnl(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(pnl, Uint128::zero());
}

#[test]
fn test_close_safe_position() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        engine,
        usdc,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(50u64),
            to_decimals(2u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // retrieve the vamm state
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::from(11_111_111_112u128));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(10u64),
            to_decimals(6u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let state = vamm.state(&router).unwrap();
    assert_eq!(state.quote_asset_reserve, to_decimals(960));
    assert_eq!(state.base_asset_reserve, Uint128::from(104_166_666_668u128));

    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());
    assert_eq!(position.margin, Uint128::zero());
    assert_eq!(position.notional, Uint128::zero());

    let state = vamm.state(&router).unwrap();
    assert_eq!(
        state.quote_asset_reserve,
        Uint128::from(1_074_626_865_681u128)
    );
    assert_eq!(state.base_asset_reserve, Uint128::from(93_055_555_556u128));

    // alice balance should be 4985.373134319
    let engine_balance = usdc.balance(&router, alice.clone()).unwrap();
    assert_eq!(engine_balance, Uint128::from(4_985_373_134_319u128));
}

#[test]
fn test_close_position_over_maintenance_margin_ration() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, to_decimals(20));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            Uint128::from(35_080_000_000u128),
            to_decimals(1u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());

    let state = vamm.state(&router).unwrap();
    assert_eq!(
        state.quote_asset_reserve,
        Uint128::from(977_422_074_621u128)
    );
    assert_eq!(state.base_asset_reserve, Uint128::from(102_309_946_334u128));
}

#[test]
fn test_close_under_collateral_position() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        insurance,
        engine,
        vamm,
        usdc,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, to_decimals(20));

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(250u64),
            to_decimals(1u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    // Now Alice's position is {balance: 20, margin: 25}
    // positionValue of 20 quoteAsset is 166.67 now
    // marginRatio = (margin(25) + unrealizedPnl(166.67-250)) / openNotionalSize(250) = -23%
    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // Alice's realizedPnl = 166.66 - 250 = -83.33, she lost all her margin(25)
    // alice.balance = all(5000) - margin(25) = 4975
    // insuranceFund.balance = 5000 + realizedPnl(-58.33) = 4941.66...
    // clearingHouse.balance = 250 + +25 + 58.33(pnl from insuranceFund) = 333.33
    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, Uint128::zero());

    // alice balance should be 4975
    let alice_balance = usdc.balance(&router, alice.clone()).unwrap();
    assert_eq!(alice_balance, Uint128::from(4_975_000_000_000u128));

    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, Uint128::from(4_941_666_666_666u128));

    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, Uint128::from(333_333_333_334u128));
}

// #[test]
// fn test_close_zero_position() {
//     let SimpleScenario {
//         mut router,
//         alice,
//         bob,
//         engine,
//         vamm,
//         usdc,
//         ..
//     } = SimpleScenario::new();

//     let msg = engine.close_position(vamm.addr().to_string()).unwrap();
//     router.execute(alice.clone(), msg).unwrap();

// }

#[test]
fn test_openclose_position_to_check_fee_is_charged() {
    let SimpleScenario {
        mut router,
        alice,
        owner,
        insurance,
        fee_pool,
        engine,
        vamm,
        usdc,
        ..
    } = SimpleScenario::new();

    let msg = vamm
        .update_config(
            None,
            Some(Uint128::from(10_000_000u128)), // 0.01
            Some(Uint128::from(20_000_000u128)), // 0.01
        )
        .unwrap();
    router.execute(owner.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, to_decimals(60u64));

    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, to_decimals(0u64));

    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, to_decimals(5024u64));

    let fee_pool_balance = usdc.balance(&router, fee_pool.clone()).unwrap();
    assert_eq!(fee_pool_balance, to_decimals(12u64));
}

#[test]
fn test_pnl_unrealized() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    // Alice long by 25 base token with leverage 10x to get 20 ptoken
    // 25 * 10 = 250 which is x
    // (1000 + 250) * (100 + y) = 1000 * 100
    // so y = -20
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(25u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    // Bob's balance in clearingHouse: 2000
    // current equation is:
    // (1250 + x) * (80 + y) = 1000 * 100
    // Bob short by 100 base token with leverage 10x to get -320 ptoken
    // 100 * 10 = 1000 which is x
    // (1250 - 1000) * (80 + y) = 1000 * 100
    // so y = 320
    //
    // and current equation is :
    // (250 + x) * (400 + y) = 1000 * 100
    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(100u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let position: PositionResponse = engine
        .position(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(position.size, to_decimals(20u64));

    // calculate Alice's unrealized PNL:
    // Alice has position 20 ptoken, so
    // (250 + x) * (400 + 20) = 1000 * 100
    // x = -11.9047619048
    // alice will get 11.9047619048 if she close position
    // since Alice use 250 to buy
    // 11.9047619048 - 250 = -238.0952380952 which is unrealized PNL.
    let pnl = engine
        .unrealized_pnl(&router, vamm.addr().to_string(), alice.to_string())
        .unwrap();
    assert_eq!(pnl, Uint128::from(238_095_238_096u64));

    // TODO return indication where it is Profit or Loss
}

#[test]
fn test_error_open_position_insufficient_balance() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        usdc,
        ..
    } = SimpleScenario::new();

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

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(10u64),
        )
        .unwrap();
    let res = router.execute(alice.clone(), msg).unwrap_err();
    assert_eq!(
        res.to_string(),
        "Overflow: Cannot Sub with 40000000000 and 120000000000".to_string()
    );
}

#[test]
fn test_error_open_position_exceed_margin_ratio() {
    let SimpleScenario {
        mut router,
        alice,
        engine,
        vamm,
        ..
    } = SimpleScenario::new();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::BUY,
            to_decimals(60u64),
            to_decimals(21u64),
        )
        .unwrap();
    let res = router.execute(alice.clone(), msg).unwrap_err();
    assert_eq!(
        res.to_string(),
        "Generic error: Position is undercollateralized".to_string()
    );
}

#[test]
fn test_alice_take_profit_from_bob_unrealized_undercollateralized_position_bob_closes() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        insurance,
        engine,
        usdc,
        vamm,
        ..
    } = SimpleScenario::new();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1980),
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
                amount: to_decimals(1980),
                expires: None,
            },
            &[],
        )
        .unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    // alice close position, pnl = 200 -105.88 ~= 94.12
    // receive pnl + margin = 114.12
    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let alice_balance = usdc.balance(&router, alice.clone()).unwrap();
    assert_eq!(alice_balance, Uint128::from(5_094_117_647_059u128));

    // bob close her under collateral position, positionValue is -294.11
    // bob's pnl = 200 - 294.11 ~= -94.12
    // bob loss all his margin (20) with additional 74.12 badDebt
    // which is already prepaid by insurance fund when alice close the position before
    // clearing house doesn't need to ask insurance fund for covering the bad debt
    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(bob.clone(), msg).unwrap();

    let bob_balance = usdc.balance(&router, bob.clone()).unwrap();
    assert_eq!(bob_balance, Uint128::from(4_980_000_000_000u128));

    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    // TODO should be zero but the is slight rounding error will investigate later
    assert_eq!(engine_balance, Uint128::zero());

    let insurance_balance = usdc.balance(&router, insurance.clone()).unwrap();
    assert_eq!(insurance_balance, Uint128::from(4_925_882_352_941u128));
}

#[test]
fn test_alice_take_profit_from_bob_unrealized_undercollateralized_position_bob_liquidated() {
    let SimpleScenario {
        mut router,
        alice,
        bob,
        carol,
        engine,
        usdc,
        vamm,
        ..
    } = SimpleScenario::new();

    // reduce the allowance
    router
        .execute_contract(
            alice.clone(),
            usdc.addr().clone(),
            &Cw20ExecuteMsg::DecreaseAllowance {
                spender: engine.addr().to_string(),
                amount: to_decimals(1980),
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
                amount: to_decimals(1980),
                expires: None,
            },
            &[],
        )
        .unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let msg = engine
        .open_position(
            vamm.addr().to_string(),
            Side::SELL,
            to_decimals(20u64),
            to_decimals(10u64),
        )
        .unwrap();
    router.execute(bob.clone(), msg).unwrap();

    // alice close position, pnl = 200 -105.88 ~= 94.12
    // receive pnl + margin = 114.12
    let msg = engine.close_position(vamm.addr().to_string()).unwrap();
    router.execute(alice.clone(), msg).unwrap();

    let alice_balance = usdc.balance(&router, alice.clone()).unwrap();
    assert_eq!(alice_balance, Uint128::from(5_094_117_647_059u128));

    // keeper liquidate bob's under collateral position, bob's positionValue is -294.11
    // bob's pnl = 200 - 294.11 ~= -94.12
    // bob loss all his margin (20) and there's 74.12 badDebt
    // which is already prepaid by insurance fund when alice close the position
    let margin_ratio = engine
        .get_margin_ratio(&router, vamm.addr().to_string(), bob.to_string())
        .unwrap();
    assert_eq!(margin_ratio.ratio, Uint128::from(252_000_000u128));
    assert_eq!(margin_ratio.polarity, false);

    // bob close her under collateral position, positionValue is -294.11
    // bob's pnl = 200 - 294.11 ~= -94.12
    // bob loss all his margin (20) with additional 74.12 badDebt
    // which is already prepaid by insurance fund when alice close the position before
    // clearing house doesn't need to ask insurance fund for covering the bad debt
    let msg = engine
        .liquidate(vamm.addr().to_string(), bob.to_string())
        .unwrap();
    router.execute(carol.clone(), msg).unwrap();

    let carol_balance = usdc.balance(&router, carol.clone()).unwrap();
    assert_eq!(carol_balance, Uint128::from(5_007_352_941_176u128));

    let engine_balance = usdc.balance(&router, engine.addr().clone()).unwrap();
    assert_eq!(engine_balance, to_decimals(0u64));
}
