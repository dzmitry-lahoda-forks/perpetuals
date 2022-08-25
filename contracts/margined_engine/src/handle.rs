use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, ReplyOn, Response, StdError, StdResult,
    SubMsg, Uint128, WasmMsg,
};

use crate::{
    contract::{
        CLOSE_POSITION_REPLY_ID, DECREASE_POSITION_REPLY_ID, INCREASE_POSITION_REPLY_ID,
        LIQUIDATION_REPLY_ID, PARTIAL_LIQUIDATION_REPLY_ID, PAY_FUNDING_REPLY_ID,
        REVERSE_POSITION_REPLY_ID,
    },
    messages::{execute_transfer_from, withdraw},
    querier::query_vamm_output_amount,
    query::{query_free_collateral, query_margin_ratio},
    state::{
        read_config, read_position, read_state, store_config, store_position, store_sent_funds,
        store_state, store_tmp_liquidator, store_tmp_swap, Config, SentFunds, State, TmpSwapInfo,
    },
    utils::{
        calc_remain_margin_with_funding_payment, direction_to_side, get_asset, get_position,
        get_position_notional_unrealized_pnl, require_additional_margin, require_bad_debt,
        require_insufficient_margin, require_non_zero_input, require_not_paused,
        require_not_restriction_mode, require_position_not_zero, require_vamm, side_to_direction,
    },
};
use margined_common::{
    asset::{Asset, AssetInfo},
    integer::Integer,
    validate::{validate_decimal_places, validate_eligible_collateral, validate_ratio},
};
use margined_perp::margined_engine::{
    PnlCalcOption, Position, PositionUnrealizedPnlResponse, Side,
};
use margined_perp::margined_vamm::{Direction, ExecuteMsg};

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    insurance_fund: Option<String>,
    fee_pool: Option<String>,
    eligible_collateral: Option<String>,
    initial_margin_ratio: Option<Uint128>,
    maintenance_margin_ratio: Option<Uint128>,
    partial_liquidation_margin_ratio: Option<Uint128>,
    liquidation_fee: Option<Uint128>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;

    // check permission
    if info.sender != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    // change owner of engine
    if let Some(owner) = owner {
        config.owner = deps.api.addr_validate(owner.as_str())?;
    }

    // update insurance fund - note altering insurance fund could lead to vAMMs being unusable maybe make this a migration
    if let Some(insurance_fund) = insurance_fund {
        config.insurance_fund = deps.api.addr_validate(insurance_fund.as_str())?;
    }

    // update fee pool
    if let Some(fee_pool) = fee_pool {
        config.fee_pool = deps.api.addr_validate(fee_pool.as_str())?;
    }

    // update eligible collaterals and therefore also decimals
    if let Some(eligible_collateral) = eligible_collateral {
        // validate eligible collateral
        config.eligible_collateral =
            validate_eligible_collateral(deps.as_ref(), eligible_collateral)?;

        // find decimals of asset
        let decimal_response = config.eligible_collateral.get_decimals(deps.as_ref())?;

        // validate decimal places are correct, and return ratio max.
        config.decimals = validate_decimal_places(decimal_response)?;
    }

    // update initial margin ratio
    if let Some(initial_margin_ratio) = initial_margin_ratio {
        validate_ratio(initial_margin_ratio, config.decimals)?;
        config.initial_margin_ratio = initial_margin_ratio;
    }

    // update maintenance margin ratio
    if let Some(maintenance_margin_ratio) = maintenance_margin_ratio {
        validate_ratio(maintenance_margin_ratio, config.decimals)?;
        config.maintenance_margin_ratio = maintenance_margin_ratio;
    }

    // update partial liquidation ratio
    if let Some(partial_liquidation_margin_ratio) = partial_liquidation_margin_ratio {
        validate_ratio(partial_liquidation_margin_ratio, config.decimals)?;
        config.partial_liquidation_margin_ratio = partial_liquidation_margin_ratio;
    }

    // update liquidation fee
    if let Some(liquidation_fee) = liquidation_fee {
        validate_ratio(liquidation_fee, config.decimals)?;
        config.liquidation_fee = liquidation_fee;
    }

    store_config(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", "update_config"))
}

pub fn set_pause(deps: DepsMut, _env: Env, info: MessageInfo, pause: bool) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let mut state: State = read_state(deps.storage)?;

    // check permission and if state matches
    if info.sender != config.owner || state.pause == pause {
        return Err(StdError::generic_err("unauthorized"));
    }

    state.pause = pause;

    store_state(deps.storage, &state)?;

    Ok(Response::default().add_attribute("action", "set_pause"))
}

// Opens a position
#[allow(clippy::too_many_arguments)]
pub fn open_position(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    side: Side,
    quote_asset_amount: Uint128,
    leverage: Uint128,
    base_asset_limit: Uint128,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let state: State = read_state(deps.storage)?;

    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = deps.api.addr_validate(info.sender.as_ref())?;

    require_not_paused(state.pause)?;
    require_vamm(deps.as_ref(), &config.insurance_fund, &vamm)?;
    require_not_restriction_mode(deps.storage, &vamm, &trader, env.block.height)?;
    require_non_zero_input(leverage)?;

    // calculate the margin ratio of new position wrt to leverage
    let margin_ratio = config
        .decimals
        .checked_mul(config.decimals)?
        .checked_div(leverage)?;
    require_additional_margin(margin_ratio, config.initial_margin_ratio)?;

    // retrieves existing position or creates a new one
    let position: Position = get_position(env.clone(), deps.storage, &vamm, &trader, side.clone());

    // if direction and side are same way then increasing else we are reversing
    let is_increase: bool = position.direction == Direction::AddToAmm && side == Side::Buy
        || position.direction == Direction::RemoveFromAmm && side == Side::Sell;

    // calculate the position notional
    let open_notional = quote_asset_amount
        .checked_mul(leverage)?
        .checked_div(config.decimals)?;

    // check if the position is new or being increased, else position is being reversed
    let msg: SubMsg = if is_increase {
        internal_increase_position(vamm.clone(), side.clone(), open_notional, base_asset_limit)
            .unwrap()
    } else {
        open_reverse_position(
            &deps,
            env,
            vamm.clone(),
            trader.clone(),
            side.clone(),
            quote_asset_amount,
            leverage,
            base_asset_limit,
            false,
        )
        .unwrap()
    };

    let PositionUnrealizedPnlResponse {
        position_notional,
        unrealized_pnl,
    } = get_position_notional_unrealized_pnl(deps.as_ref(), &position, PnlCalcOption::SpotPrice)
        .unwrap();

    store_tmp_swap(
        deps.storage,
        &TmpSwapInfo {
            vamm: vamm.clone(),
            trader: trader.clone(),
            side,
            quote_asset_amount,
            leverage,
            open_notional,
            position_notional,
            unrealized_pnl,
            margin_to_vault: Integer::zero(),
            fees_paid: false,
        },
    )?;

    store_sent_funds(
        deps.storage,
        &SentFunds {
            asset: get_asset(info, config.eligible_collateral),
            required: Uint128::zero(),
        },
    )?;

    Ok(Response::new().add_submessage(msg).add_attributes(vec![
        ("action", "open_position"),
        ("vamm", vamm.as_ref()),
        ("trader", trader.as_ref()),
        ("quote_asset_amount", &quote_asset_amount.to_string()),
        ("leverage", &leverage.to_string()),
    ]))
}

pub fn close_position(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    quote_amount_limit: Uint128,
) -> StdResult<Response> {
    let state: State = read_state(deps.storage)?;

    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = deps.api.addr_validate(info.sender.as_ref())?;

    // read the position for the trader from vamm
    let position = read_position(deps.storage, &vamm, &trader).unwrap();

    // check the position isn't zero
    require_not_paused(state.pause)?;
    require_position_not_zero(position.size.value)?;
    require_not_restriction_mode(deps.storage, &vamm, &trader, env.block.height)?;

    let msg =
        internal_close_position(deps, &position, quote_amount_limit, CLOSE_POSITION_REPLY_ID)?;

    Ok(Response::new().add_submessage(msg).add_attributes(vec![
        ("action", "close_position"),
        ("vamm", vamm.as_ref()),
        ("trader", trader.as_ref()),
    ]))
}

pub fn liquidate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    trader: String,
    quote_asset_limit: Uint128,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;

    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = deps.api.addr_validate(&trader)?;

    // store the liquidator
    store_tmp_liquidator(deps.storage, &info.sender)?;

    // retrieve the existing margin ratio of the position
    let margin_ratio = query_margin_ratio(deps.as_ref(), vamm.to_string(), trader.to_string())?;

    require_vamm(deps.as_ref(), &config.insurance_fund, &vamm)?;
    require_insufficient_margin(margin_ratio, config.maintenance_margin_ratio)?;

    // read the position for the trader from vamm
    let position = read_position(deps.storage, &vamm, &trader).unwrap();

    // check the position isn't zero
    require_position_not_zero(position.size.value)?;

    // first see if this is a partial liquidation, else get rekt
    let msg = if margin_ratio.value > config.liquidation_fee
        && !config.partial_liquidation_margin_ratio.is_zero()
    {
        partial_liquidation(deps, env, vamm.clone(), trader.clone(), quote_asset_limit)?
    } else {
        internal_close_position(deps, &position, quote_asset_limit, LIQUIDATION_REPLY_ID)?
    };

    Ok(Response::new().add_submessage(msg).add_attributes(vec![
        ("action", "liquidate"),
        ("vamm", vamm.as_ref()),
        ("trader", trader.as_ref()),
    ]))
}

/// settles funding in amm specified
pub fn pay_funding(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    vamm: String,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;

    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;

    // check its a valid vamm
    require_vamm(deps.as_ref(), &config.insurance_fund, &vamm)?;

    let funding_msg = SubMsg {
        msg: CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: vamm.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::SettleFunding {})?,
        }),
        gas_limit: None,
        id: PAY_FUNDING_REPLY_ID,
        reply_on: ReplyOn::Always,
    };

    Ok(Response::new()
        .add_submessage(funding_msg)
        .add_attribute("action", "pay_funding"))
}

/// Enables a user to directly deposit margin into their position
pub fn deposit_margin(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    amount: Uint128,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let state: State = read_state(deps.storage)?;

    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = info.sender.clone();

    require_not_paused(state.pause)?;
    require_non_zero_input(amount)?;

    // first try to execute the transfer
    let mut response: Response = Response::new();
    match config.eligible_collateral.clone() {
        AssetInfo::NativeToken { .. } => {
            let token = Asset {
                info: config.eligible_collateral,
                amount,
            };

            token.assert_sent_native_token_balance(&info)?;
        }

        AssetInfo::Token { .. } => {
            let msg: SubMsg =
                execute_transfer_from(deps.storage, &trader, &env.contract.address, amount)?;
            response = response.clone().add_submessage(msg);
        }
    };

    // read the position for the trader from vamm
    let mut position = read_position(deps.storage, &vamm, &trader).unwrap();
    position.margin = position.margin.checked_add(amount)?;

    store_position(deps.storage, &position)?;

    Ok(response.add_attributes([
        ("action", "deposit_margin"),
        ("trader", trader.as_ref()),
        ("deposit_amount", &amount.to_string()),
    ]))
}

/// Enables a user to directly withdraw excess margin from their position
pub fn withdraw_margin(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    amount: Uint128,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let mut state: State = read_state(deps.storage)?;

    // get and validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = info.sender;

    require_vamm(deps.as_ref(), &config.insurance_fund, &vamm)?;
    require_not_paused(state.pause)?;
    require_non_zero_input(amount)?;

    // read the position for the trader from vamm
    let mut position = read_position(deps.storage, &vamm, &trader).unwrap();

    let remain_margin = calc_remain_margin_with_funding_payment(
        deps.as_ref(),
        position.clone(),
        Integer::new_negative(amount),
    )?;
    require_bad_debt(remain_margin.bad_debt)?;

    position.margin = remain_margin.margin;
    position.last_updated_premium_fraction = remain_margin.latest_premium_fraction;

    // check if margin is sufficient
    let free_collateral =
        query_free_collateral(deps.as_ref(), vamm.to_string(), trader.to_string())?;
    if free_collateral
        .checked_sub(Integer::new_positive(amount))?
        .is_negative()
    {
        return Err(StdError::generic_err("Insufficient collateral"));
    }

    store_position(deps.storage, &position)?;

    // withdraw margin
    let msgs = withdraw(
        deps.as_ref(),
        env,
        &mut state,
        &trader,
        config.eligible_collateral,
        amount,
    )
    .unwrap();

    Ok(Response::new().add_submessages(msgs).add_attributes(vec![
        ("action", "withdraw_margin"),
        ("trader", trader.as_ref()),
        ("withdrawal_amount", &amount.to_string()),
    ]))
}

// Increase the position through a swap
pub fn internal_increase_position(
    vamm: Addr,
    side: Side,
    open_notional: Uint128,
    base_asset_limit: Uint128,
) -> StdResult<SubMsg> {
    swap_input(
        &vamm,
        side,
        open_notional,
        base_asset_limit,
        false,
        INCREASE_POSITION_REPLY_ID,
    )
}

pub fn internal_close_position(
    deps: DepsMut,
    position: &Position,
    quote_asset_limit: Uint128,
    id: u64,
) -> StdResult<SubMsg> {
    store_tmp_swap(
        deps.storage,
        &TmpSwapInfo {
            vamm: position.vamm.clone(),
            trader: position.trader.clone(),
            side: direction_to_side(position.direction.clone()),
            quote_asset_amount: position.size.value,
            leverage: Uint128::zero(),
            open_notional: position.notional,
            position_notional: Uint128::zero(),
            unrealized_pnl: Integer::zero(),
            margin_to_vault: Integer::zero(),
            fees_paid: false,
        },
    )?;

    swap_output(
        &position.vamm.clone(),
        direction_to_side(position.direction.clone()),
        position.size.value,
        quote_asset_limit,
        id,
    )
}

#[allow(clippy::too_many_arguments)]
fn open_reverse_position(
    deps: &DepsMut,
    env: Env,
    vamm: Addr,
    trader: Addr,
    side: Side,
    quote_asset_amount: Uint128,
    leverage: Uint128,
    base_asset_limit: Uint128,
    can_go_over_fluctuation: bool,
) -> StdResult<SubMsg> {
    let config: Config = read_config(deps.storage).unwrap();
    let position: Position = get_position(env, deps.storage, &vamm, &trader, side.clone());

    // calc the input amount wrt to leverage and decimals
    let open_notional = quote_asset_amount
        .checked_mul(leverage)
        .unwrap()
        .checked_div(config.decimals)
        .unwrap();

    let PositionUnrealizedPnlResponse {
        position_notional,
        unrealized_pnl: _,
    } = get_position_notional_unrealized_pnl(deps.as_ref(), &position, PnlCalcOption::SpotPrice)
        .unwrap();

    // reduce position if old position is larger
    let msg: SubMsg = if position_notional > open_notional {
        // then we are opening a new position or adding to an existing
        swap_input(
            &vamm,
            side,
            open_notional,
            base_asset_limit,
            can_go_over_fluctuation,
            DECREASE_POSITION_REPLY_ID,
        )
        .unwrap()
    } else {
        // first close position swap out the entire position
        swap_output(
            &vamm,
            direction_to_side(position.direction.clone()),
            position.size.value,
            Uint128::zero(),
            REVERSE_POSITION_REPLY_ID,
        )
        .unwrap()
    };

    Ok(msg)
}

fn partial_liquidation(
    deps: DepsMut,
    _env: Env,
    vamm: Addr,
    trader: Addr,
    quote_asset_limit: Uint128,
) -> StdResult<SubMsg> {
    let config: Config = read_config(deps.storage).unwrap();

    let position: Position = read_position(deps.storage, &vamm, &trader).unwrap();

    let partial_position_size = position
        .size
        .value
        .checked_mul(config.partial_liquidation_margin_ratio)
        .unwrap()
        .checked_div(config.decimals)
        .unwrap();

    let partial_asset_limit = quote_asset_limit
        .checked_mul(config.partial_liquidation_margin_ratio)
        .unwrap()
        .checked_div(config.decimals)
        .unwrap();

    let current_notional = query_vamm_output_amount(
        &deps.as_ref(),
        vamm.to_string(),
        position.direction.clone(),
        partial_position_size,
    )
    .unwrap();

    let PositionUnrealizedPnlResponse {
        position_notional: _,
        unrealized_pnl,
    } = get_position_notional_unrealized_pnl(deps.as_ref(), &position, PnlCalcOption::SpotPrice)
        .unwrap();

    let side = if position.size > Integer::zero() {
        Side::Sell
    } else {
        Side::Buy
    };

    store_tmp_swap(
        deps.storage,
        &TmpSwapInfo {
            vamm: position.vamm.clone(),
            trader: position.trader.clone(),
            side,
            quote_asset_amount: partial_position_size,
            leverage: Uint128::zero(),
            open_notional: current_notional,
            position_notional: Uint128::zero(),
            unrealized_pnl,
            margin_to_vault: Integer::zero(),
            fees_paid: false,
        },
    )
    .unwrap();

    let msg: SubMsg = if current_notional > position.notional {
        swap_input(
            &vamm,
            direction_to_side(position.direction.clone()),
            position.notional,
            Uint128::zero(),
            true,
            PARTIAL_LIQUIDATION_REPLY_ID,
        )
        .unwrap()
    } else {
        swap_output(
            &vamm,
            direction_to_side(position.direction),
            partial_position_size,
            partial_asset_limit,
            PARTIAL_LIQUIDATION_REPLY_ID,
        )
        .unwrap()
    };

    Ok(msg)
}

fn swap_input(
    vamm: &Addr,
    side: Side,
    open_notional: Uint128,
    base_asset_limit: Uint128,
    can_go_over_fluctuation: bool,
    id: u64,
) -> StdResult<SubMsg> {
    let direction: Direction = side_to_direction(side);

    let msg = WasmMsg::Execute {
        contract_addr: vamm.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::SwapInput {
            direction,
            quote_asset_amount: open_notional,
            base_asset_limit,
            can_go_over_fluctuation,
        })?,
    };

    let execute_submsg = SubMsg {
        msg: CosmosMsg::Wasm(msg),
        gas_limit: None,
        id,
        reply_on: ReplyOn::Always,
    };

    Ok(execute_submsg)
}

fn swap_output(
    vamm: &Addr,
    side: Side,
    open_notional: Uint128,
    quote_asset_limit: Uint128,
    id: u64,
) -> StdResult<SubMsg> {
    let direction: Direction = side_to_direction(side);

    let swap_msg = WasmMsg::Execute {
        contract_addr: vamm.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::SwapOutput {
            direction,
            base_asset_amount: open_notional,
            quote_asset_limit,
        })?,
    };

    let execute_submsg = SubMsg {
        msg: CosmosMsg::Wasm(swap_msg),
        gas_limit: None,
        id,
        reply_on: ReplyOn::Always,
    };

    Ok(execute_submsg)
}
