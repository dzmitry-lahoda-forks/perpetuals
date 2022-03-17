use cosmwasm_std::{Addr, Empty, Uint128};
use cw20::{Cw20Coin, Cw20Contract, Cw20ExecuteMsg};
use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};
use margined_perp::margined_engine::InstantiateMsg;
use margined_perp::margined_pricefeed::InstantiateMsg as PricefeedInstantiateMsg;
use margined_perp::margined_vamm::InstantiateMsg as VammInstantiateMsg;

use crate::contracts::helpers::{
    margined_engine::EngineController, margined_pricefeed::PricefeedController,
    margined_vamm::VammController,
};

pub struct ContractInfo {
    pub addr: Addr,
    pub id: u64,
}

pub struct SimpleScenario {
    pub router: App,
    pub owner: Addr,
    pub alice: Addr,
    pub bob: Addr,
    pub carol: Addr,
    pub insurance: Addr,
    pub fee_pool: Addr,
    pub usdc: Cw20Contract,
    pub vamm: VammController,
    pub engine: EngineController,
    pub pricefeed: PricefeedController,
}

impl SimpleScenario {
    pub fn new() -> Self {
        let mut router: App = AppBuilder::new().build();

        let owner = Addr::unchecked("owner");
        let alice = Addr::unchecked("alice");
        let bob = Addr::unchecked("bob");
        let carol = Addr::unchecked("carol");
        let insurance_fund = Addr::unchecked("insurance_fund");
        let fee_pool = Addr::unchecked("fee_pool");

        let usdc_id = router.store_code(contract_cw20());
        let engine_id = router.store_code(contract_engine());
        let vamm_id = router.store_code(contract_vamm());
        let pricefeed_id = router.store_code(contract_pricefeed());

        let usdc_addr = router
            .instantiate_contract(
                usdc_id,
                owner.clone(),
                &cw20_base::msg::InstantiateMsg {
                    name: "USDC".to_string(),
                    symbol: "USDC".to_string(),
                    decimals: 9,
                    initial_balances: vec![
                        Cw20Coin {
                            address: alice.to_string(),
                            amount: to_decimals(5000),
                        },
                        Cw20Coin {
                            address: bob.to_string(),
                            amount: to_decimals(5000),
                        },
                        Cw20Coin {
                            address: carol.to_string(),
                            amount: to_decimals(5000),
                        },
                        Cw20Coin {
                            address: insurance_fund.to_string(),
                            amount: to_decimals(5000),
                        },
                    ],
                    mint: None,
                    marketing: None,
                },
                &[],
                "cw20",
                None,
            )
            .unwrap();

        let usdc = Cw20Contract(usdc_addr.clone());

        let pricefeed_addr = router
            .instantiate_contract(
                pricefeed_id,
                owner.clone(),
                &PricefeedInstantiateMsg {
                    decimals: 9u8,
                    oracle_hub_contract: "oracle_hub0000".to_string(),
                },
                &[],
                "pricefeed",
                None,
            )
            .unwrap();
        let pricefeed = PricefeedController(pricefeed_addr.clone());

        let vamm_addr = router
            .instantiate_contract(
                vamm_id,
                owner.clone(),
                &VammInstantiateMsg {
                    decimals: 9u8,
                    quote_asset: "ETH".to_string(),
                    base_asset: "USD".to_string(),
                    quote_asset_reserve: to_decimals(1_000),
                    base_asset_reserve: to_decimals(100),
                    funding_period: 3_600_u64,
                    toll_ratio: Uint128::zero(),
                    spread_ratio: Uint128::zero(),
                    pricefeed: pricefeed_addr.to_string(),
                },
                &[],
                "vamm",
                None,
            )
            .unwrap();
        let vamm = VammController(vamm_addr.clone());

        // set up margined engine contract
        let engine_addr = router
            .instantiate_contract(
                engine_id,
                owner.clone(),
                &InstantiateMsg {
                    decimals: 9u8,
                    insurance_fund: insurance_fund.to_string(),
                    fee_pool: fee_pool.to_string(),
                    eligible_collateral: usdc_addr.to_string(),
                    initial_margin_ratio: Uint128::from(50_000_000u128), // 0.05
                    maintenance_margin_ratio: Uint128::from(50_000_000u128), // 0.05
                    liquidation_fee: Uint128::from(50_000_000u128),      // 0.05
                    vamm: vec![vamm_addr.to_string()],
                },
                &[],
                "engine",
                None,
            )
            .unwrap();
        let engine = EngineController(engine_addr.clone());

        // create allowance for alice
        router
            .execute_contract(
                alice.clone(),
                usdc_addr.clone(),
                &Cw20ExecuteMsg::IncreaseAllowance {
                    spender: engine_addr.to_string(),
                    amount: to_decimals(2000),
                    expires: None,
                },
                &[],
            )
            .unwrap();

        // create allowance for bob
        router
            .execute_contract(
                bob.clone(),
                usdc_addr.clone(),
                &Cw20ExecuteMsg::IncreaseAllowance {
                    spender: engine_addr.to_string(),
                    amount: to_decimals(2000),
                    expires: None,
                },
                &[],
            )
            .unwrap();

        // create allowance for insurance_fund
        router
            .execute_contract(
                insurance_fund.clone(),
                usdc_addr,
                &Cw20ExecuteMsg::IncreaseAllowance {
                    spender: engine_addr.to_string(),
                    amount: to_decimals(2000),
                    expires: None,
                },
                &[],
            )
            .unwrap();

        Self {
            router,
            owner,
            alice,
            bob,
            carol,
            insurance: insurance_fund,
            fee_pool,
            usdc,
            pricefeed,
            vamm,
            engine,
        }
    }
}

impl Default for SimpleScenario {
    fn default() -> Self {
        SimpleScenario::new()
    }
}

pub const DECIMAL_MULTIPLIER: Uint128 = Uint128::new(1_000_000_000);

fn contract_cw20() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    );
    Box::new(contract)
}

fn contract_vamm() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        margined_vamm::contract::execute,
        margined_vamm::contract::instantiate,
        margined_vamm::contract::query,
    );
    Box::new(contract)
}

fn contract_engine() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        margined_engine::contract::execute,
        margined_engine::contract::instantiate,
        margined_engine::contract::query,
    )
    .with_reply(margined_engine::contract::reply);
    Box::new(contract)
}

fn contract_pricefeed() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        margined_pricefeed::contract::execute,
        margined_pricefeed::contract::instantiate,
        margined_pricefeed::contract::query,
    );
    Box::new(contract)
}

// takes in a Uint128 and multiplies by the decimals just to make tests more legible
pub fn to_decimals(input: u64) -> Uint128 {
    Uint128::from(input) * DECIMAL_MULTIPLIER
}
