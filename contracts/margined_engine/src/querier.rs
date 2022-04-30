// Contains queries for external contracts,
use cosmwasm_std::{to_binary, Deps, QueryRequest, StdResult, Uint128, WasmQuery};

use margined_perp::{
    margined_insurance_fund::{QueryMsg as InsuranceFundQueryMsg, VammResponse},
    margined_vamm::{CalcFeeResponse, ConfigResponse, Direction, QueryMsg, StateResponse},
};

// returns the config of the request vamm
pub fn query_vamm_config(deps: &Deps, address: String) -> StdResult<ConfigResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: address,
        msg: to_binary(&QueryMsg::Config {})?,
    }))
}

// returns the state of the request vamm
// can be used to calculate the input and outputs
pub fn query_vamm_state(deps: &Deps, address: String) -> StdResult<StateResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: address,
        msg: to_binary(&QueryMsg::State {})?,
    }))
}

// returns the state of the request vamm
// can be used to calculate the input and outputs
pub fn query_vamm_output_price(
    deps: &Deps,
    address: String,
    direction: Direction,
    amount: Uint128,
) -> StdResult<Uint128> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: address,
        msg: to_binary(&QueryMsg::OutputPrice { direction, amount })?,
    }))
}

// returns the state of the request vamm
// can be used to calculate the input and outputs
pub fn query_vamm_output_twap(
    deps: &Deps,
    address: String,
    direction: Direction,
    amount: Uint128,
) -> StdResult<Uint128> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: address,
        msg: to_binary(&QueryMsg::OutputTwap { direction, amount })?,
    }))
}

// returns the spread and toll fees
pub fn query_vamm_calc_fee(
    deps: &Deps,
    address: String,
    quote_asset_amount: Uint128,
) -> StdResult<CalcFeeResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: address,
        msg: to_binary(&QueryMsg::CalcFee { quote_asset_amount })?,
    }))
}

// returns true if vamm has been registered with the insurance contract
pub fn query_insurance_is_vamm(
    deps: &Deps,
    insurance: String,
    vamm: String,
) -> StdResult<VammResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: insurance,
        msg: to_binary(&InsuranceFundQueryMsg::IsVamm { vamm })?,
    }))
}
