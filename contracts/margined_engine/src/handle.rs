use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, ReplyOn, Response, StdError, StdResult,
    SubMsg, Uint128, WasmMsg,
};

use crate::{
    contract::{
        PAY_FUNDING_REPLY_ID, SWAP_CLOSE_REPLY_ID, SWAP_DECREASE_REPLY_ID, SWAP_INCREASE_REPLY_ID,
        SWAP_LIQUIDATE_REPLY_ID, SWAP_PARTIAL_LIQUIDATION_REPLY_ID, SWAP_REVERSE_REPLY_ID,
    },
    querier::query_vamm_output_price,
    query::query_margin_ratio,
    state::{
        read_config, read_position, read_state, store_config, store_position, store_tmp_liquidator,
        store_tmp_swap, Config, Position, State, Swap,
    },
    utils::{
        calc_remain_margin_with_funding_payment, direction_to_side, execute_transfer_from,
        get_position, get_position_notional_unrealized_pnl, require_bad_debt,
        require_insufficient_margin, require_margin, require_position_not_zero, require_vamm,
        side_to_direction, withdraw,
    },
};
use margined_common::integer::Integer;
use margined_perp::margined_engine::{PnlCalcOption, PositionUnrealizedPnlResponse, Side};
use margined_perp::margined_vamm::{Direction, ExecuteMsg};

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    insurance_fund: Option<String>,
    fee_pool: Option<String>,
    eligible_collateral: Option<String>,
    decimals: Option<Uint128>,
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

    // change owner of amm
    if let Some(owner) = owner {
        config.owner = deps.api.addr_validate(owner.as_str())?;
    }

    // update insurance fund
    if let Some(insurance_fund) = insurance_fund {
        config.insurance_fund = deps.api.addr_validate(insurance_fund.as_str())?;
    }

    // update fee pool
    if let Some(fee_pool) = fee_pool {
        config.fee_pool = deps.api.addr_validate(fee_pool.as_str())?;
    }

    // update eligible collateral
    if let Some(eligible_collateral) = eligible_collateral {
        config.eligible_collateral = deps.api.addr_validate(eligible_collateral.as_str())?;
    }

    // update decimals TODO: remove all this
    if let Some(decimals) = decimals {
        config.decimals = decimals;
    }

    // update initial margin ratio
    if let Some(initial_margin_ratio) = initial_margin_ratio {
        config.initial_margin_ratio = initial_margin_ratio;
    }

    // update maintenance margin ratio
    if let Some(maintenance_margin_ratio) = maintenance_margin_ratio {
        config.maintenance_margin_ratio = maintenance_margin_ratio;
    }

    // update partial liquidation ratio
    if let Some(partial_liquidation_margin_ratio) = partial_liquidation_margin_ratio {
        config.partial_liquidation_margin_ratio = partial_liquidation_margin_ratio;
    }

    // update liquidation fee
    if let Some(liquidation_fee) = liquidation_fee {
        config.liquidation_fee = liquidation_fee;
    }

    store_config(deps.storage, &config)?;

    Ok(Response::default())
}

// Opens a position
// TODO - refactor arguments into a struct
#[allow(clippy::too_many_arguments)]
pub fn open_position(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    vamm: String,
    trader: String,
    side: Side,
    quote_asset_amount: Uint128,
    leverage: Uint128,
    base_asset_limit: Uint128,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;

    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = deps.api.addr_validate(&trader)?;

    let margin_ratio = Uint128::from(1_000_000_000u64)
        .checked_mul(config.decimals)?
        .checked_div(leverage)?;

    require_vamm(deps.storage, &vamm)?;
    require_margin(margin_ratio, config.initial_margin_ratio)?;

    // calc the input amount wrt to leverage and decimals
    let open_notional = quote_asset_amount
        .checked_mul(leverage)?
        .checked_div(config.decimals)?;

    let position: Position = get_position(env.clone(), deps.storage, &vamm, &trader, side.clone());

    let mut is_increase: bool = true;
    if !(position.direction == Direction::AddToAmm && side == Side::BUY
        || position.direction == Direction::RemoveFromAmm && side == Side::SELL)
    {
        is_increase = false;
    }

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
            open_notional,
            base_asset_limit,
            false,
        )
    };

    store_tmp_swap(
        deps.storage,
        &Swap {
            vamm,
            trader,
            side,
            quote_asset_amount,
            leverage,
            open_notional,
            unrealized_pnl: Integer::zero(),
        },
    )?;

    Ok(Response::new()
        .add_submessage(msg)
        .add_attributes(vec![("action", "open_position")]))
}

pub fn close_position(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    vamm: String,
    trader: String,
    quote_amount_limit: Uint128,
) -> StdResult<Response> {
    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = deps.api.addr_validate(&trader)?;

    // read the position for the trader from vamm
    let position = read_position(deps.storage, &vamm, &trader).unwrap();

    // check the position isn't zero
    require_position_not_zero(position.size.value)?;

    let msg = internal_close_position(deps, &position, quote_amount_limit, SWAP_CLOSE_REPLY_ID)?;

    Ok(Response::new().add_submessage(msg))
}

pub fn liquidate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    trader: String,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;

    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = deps.api.addr_validate(&trader)?;

    // store the liquidator
    store_tmp_liquidator(deps.storage, &info.sender)?;

    // check if margin ratio has been
    let margin_ratio = query_margin_ratio(deps.as_ref(), vamm.to_string(), trader.to_string())?;

    require_vamm(deps.storage, &vamm)?;
    require_insufficient_margin(margin_ratio, config.maintenance_margin_ratio)?;

    // read the position for the trader from vamm
    let position = read_position(deps.storage, &vamm, &trader).unwrap();

    // first see if this is a partial liquidation, else we just rek the trader
    let msg = if margin_ratio.value > config.liquidation_fee
        && !config.partial_liquidation_margin_ratio.is_zero()
    {
        partial_liquidation(deps, env, vamm, trader)
    } else {
        internal_close_position(deps, &position, Uint128::zero(), SWAP_LIQUIDATE_REPLY_ID)?
    };

    Ok(Response::default().add_submessage(msg))
}

pub fn pay_funding(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    vamm: String,
) -> StdResult<Response> {
    // validate address inputs
    let vamm = deps.api.addr_validate(&vamm)?;

    // check its a valid vamm
    require_vamm(deps.storage, &vamm)?;

    let funding_msg = SubMsg {
        msg: CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: vamm.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::SettleFunding {})?,
        }),
        gas_limit: None, // probably should set a limit in the config
        id: PAY_FUNDING_REPLY_ID,
        reply_on: ReplyOn::Always,
    };

    Ok(Response::new().add_submessage(funding_msg))
}

/// Enables a user to directly deposit margin into their position
pub fn deposit_margin(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vamm: String,
    amount: Uint128,
) -> StdResult<Response> {
    let vamm = deps.api.addr_validate(&vamm)?;
    let trader = info.sender;

    // first try to execute the transfer
    let msg = execute_transfer_from(deps.storage, &trader, &env.contract.address, amount)?;

    // read the position for the trader from vamm
    let mut position = read_position(deps.storage, &vamm, &trader).unwrap();
    position.margin = position.margin.checked_add(amount)?;

    store_position(deps.storage, &position)?;

    Ok(Response::new().add_submessage(msg).add_attributes(vec![
        ("action", "deposit_margin"),
        ("trader", &trader.to_string()),
        ("amount", &amount.to_string()),
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

    require_vamm(deps.storage, &vamm)?;

    // read the position for the trader from vamm
    let mut position = read_position(deps.storage, &vamm, &trader).unwrap();

    let margin_delta = Integer::new_negative(amount);

    let remain_margin =
        calc_remain_margin_with_funding_payment(deps.as_ref(), position.clone(), margin_delta)?;
    require_bad_debt(remain_margin.bad_debt)?;

    position.margin = remain_margin.margin;
    position.last_updated_premium_fraction = remain_margin.latest_premium_fraction;

    store_position(deps.storage, &position)?;

    // check if margin ratio has been
    let margin_ratio = query_margin_ratio(deps.as_ref(), vamm.to_string(), trader.to_string())?;

    require_margin(margin_ratio.value, config.initial_margin_ratio)?;

    // try to execute the transfer
    let msgs = withdraw(
        deps.as_ref(),
        env,
        &mut state,
        &trader,
        &config.insurance_fund,
        config.eligible_collateral,
        amount,
    )
    .unwrap();

    Ok(Response::new().add_submessages(msgs).add_attributes(vec![
        ("action", "withdraw_margin"),
        ("trader", &trader.to_string()),
        ("amount", &amount.to_string()),
    ]))
}

// Increase the position, just basically wraps swap input though it may do more in the future
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
        SWAP_INCREASE_REPLY_ID,
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
        &Swap {
            vamm: position.vamm.clone(),
            trader: position.trader.clone(),
            side: direction_to_side(position.direction.clone()),
            quote_asset_amount: position.size.value,
            leverage: Uint128::zero(),
            open_notional: position.notional,
            unrealized_pnl: Integer::zero(),
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
    open_notional: Uint128,
    base_amount_limit: Uint128,
    can_go_over_fluctuation: bool,
) -> SubMsg {
    let position: Position = get_position(env, deps.storage, &vamm, &trader, side.clone());
    let current_notional = query_vamm_output_price(
        &deps.as_ref(),
        vamm.to_string(),
        position.direction.clone(),
        position.size.value,
    )
    .unwrap();

    // if position.notional > open_notional {
    let msg: SubMsg = if current_notional > open_notional {
        // then we are opening a new position or adding to an existing
        swap_input(
            &vamm,
            side,
            open_notional,
            base_amount_limit,
            can_go_over_fluctuation,
            SWAP_DECREASE_REPLY_ID,
        )
        .unwrap()
    } else {
        // first close position swap out the entire position
        swap_output(
            &vamm,
            direction_to_side(position.direction.clone()),
            position.size.value,
            Uint128::zero(),
            SWAP_REVERSE_REPLY_ID,
        )
        .unwrap()
    };

    msg
}

#[allow(clippy::too_many_arguments)]
fn partial_liquidation(deps: DepsMut, _env: Env, vamm: Addr, trader: Addr) -> SubMsg {
    let config: Config = read_config(deps.storage).unwrap();

    let position: Position = read_position(deps.storage, &vamm, &trader).unwrap();

    let partial_position_size = position
        .size
        .value
        .checked_mul(config.partial_liquidation_margin_ratio)
        .unwrap()
        .checked_div(config.decimals)
        .unwrap();

    let current_notional = query_vamm_output_price(
        &deps.as_ref(),
        vamm.to_string(),
        position.direction.clone(),
        partial_position_size,
    )
    .unwrap();

    let PositionUnrealizedPnlResponse {
        position_notional: _,
        unrealized_pnl,
    } = get_position_notional_unrealized_pnl(deps.as_ref(), &position, PnlCalcOption::SPOTPRICE)
        .unwrap();

    let side = if position.size > Integer::zero() {
        Side::SELL
    } else {
        Side::BUY
    };

    store_tmp_swap(
        deps.storage,
        &Swap {
            vamm: position.vamm.clone(),
            trader: position.trader.clone(),
            side,
            quote_asset_amount: partial_position_size,
            leverage: Uint128::zero(),
            open_notional: current_notional,
            unrealized_pnl,
        },
    )
    .unwrap();

    // if position.notional > open_notional {
    let msg: SubMsg = if current_notional > position.notional {
        // then we are opening a new position or adding to an existing
        swap_input(
            &vamm,
            direction_to_side(position.direction.clone()),
            position.notional,
            Uint128::zero(),
            true,
            SWAP_PARTIAL_LIQUIDATION_REPLY_ID,
        )
        .unwrap()
    } else {
        // first close position swap out the entire position
        swap_output(
            &vamm,
            direction_to_side(position.direction),
            partial_position_size,
            Uint128::zero(),
            SWAP_PARTIAL_LIQUIDATION_REPLY_ID,
        )
        .unwrap()
    };

    msg
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
        gas_limit: None, // probably should set a limit in the config
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
        gas_limit: None, // probably should set a limit in the config
        id,
        reply_on: ReplyOn::Always,
    };

    Ok(execute_submsg)
}
