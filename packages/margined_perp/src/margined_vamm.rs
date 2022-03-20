use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};

use margined_common::integer::Integer;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    AddToAmm,
    RemoveFromAmm,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LongShort {
    Long,
    Short,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub decimals: u8,
    pub pricefeed: String,
    pub quote_asset: String,
    pub base_asset: String,
    pub quote_asset_reserve: Uint128,
    pub base_asset_reserve: Uint128,
    pub funding_period: u64,
    pub toll_ratio: Uint128,
    pub spread_ratio: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateConfig {
        owner: Option<String>,
        // open: Option<bool>,
        // spot_price_twap_interval: Option<Uint128>,
        toll_ratio: Option<Uint128>,
        spread_ratio: Option<Uint128>,
        margin_engine: Option<String>,
        pricefeed: Option<String>,
    },
    SwapInput {
        direction: Direction,
        quote_asset_amount: Uint128,
    },
    SwapOutput {
        direction: Direction,
        base_asset_amount: Uint128,
    },
    SettleFunding {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    OutputPrice {
        direction: Direction,
        amount: Uint128,
    },
    InputTwap {
        direction: Direction,
        amount: Uint128,
    },
    OutputTwap {
        direction: Direction,
        amount: Uint128,
    },
    // UnderlyingPrice {},
    // UnderlyingTwapPrice {},
    SpotPrice {},
    TwapPrice {
        interval: u64,
    },
    CalcFee {
        quote_asset_amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: Addr,
    pub margin_engine: Addr,
    pub pricefeed: Addr,
    pub quote_asset: String,
    pub base_asset: String,
    pub toll_ratio: Uint128,
    pub spread_ratio: Uint128,
    pub decimals: Uint128,
    pub funding_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub quote_asset_reserve: Uint128,
    pub base_asset_reserve: Uint128,
    pub total_position_size: Integer,
    pub funding_rate: Uint128,
    pub next_funding_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CalcFeeResponse {
    pub toll_fee: Uint128,
    pub spread_fee: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PremiumResponse {
    pub value: Uint128,
    pub payer: LongShort, // are the longs paying or the shorts?
}

impl Default for PremiumResponse {
    fn default() -> Self {
        PremiumResponse {
            value: Uint128::zero(),
            payer: LongShort::Long,
        }
    }
}
