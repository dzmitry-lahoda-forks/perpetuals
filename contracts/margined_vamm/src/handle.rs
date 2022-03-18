use cosmwasm_std::{
    Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Storage, Uint128,
};

use margined_common::integer::Integer;
use margined_perp::margined_vamm::{Direction, LongShort, PremiumResponse};

use crate::{
    contract::{ONE_DAY_IN_SECONDS, ONE_HOUR_IN_SECONDS},
    decimals::modulo,
    querier::query_underlying_twap_price,
    query::query_twap_price,
    state::{
        read_config, read_reserve_snapshot, read_reserve_snapshot_counter, read_state,
        store_config, store_reserve_snapshot, store_state, update_reserve_snapshot, Config,
        ReserveSnapshot, State,
    },
};

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    toll_ratio: Option<Uint128>,
    spread_ratio: Option<Uint128>,
    pricefeed: Option<String>,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage)?;

    // check permission
    if info.sender != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    // change owner of amm
    if let Some(owner) = owner {
        config.owner = deps.api.addr_validate(owner.as_str())?;
    }

    // change toll ratio
    if let Some(toll_ratio) = toll_ratio {
        config.toll_ratio = toll_ratio;
    }

    // change spread ratio
    if let Some(spread_ratio) = spread_ratio {
        config.spread_ratio = spread_ratio;
    }

    // change pricefeed
    if let Some(pricefeed) = pricefeed {
        config.pricefeed = deps.api.addr_validate(&pricefeed).unwrap();
    }

    store_config(deps.storage, &config)?;

    Ok(Response::default())
}

// Function should only be called by the margin engine
pub fn swap_input(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    direction: Direction,
    quote_asset_amount: Uint128,
) -> StdResult<Response> {
    let state: State = read_state(deps.storage)?;

    let base_asset_amount = get_input_price_with_reserves(
        deps.as_ref(),
        &direction,
        quote_asset_amount,
        state.quote_asset_reserve,
        state.base_asset_reserve,
    )?;

    update_reserve(
        deps.storage,
        env,
        direction,
        quote_asset_amount,
        base_asset_amount,
    )?;

    Ok(Response::new().add_attributes(vec![
        ("action", "swap_input"),
        ("input", &quote_asset_amount.to_string()),
        ("output", &base_asset_amount.to_string()),
    ]))
}

// Function should only be called by the margin engine
pub fn swap_output(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    direction: Direction,
    base_asset_amount: Uint128,
) -> StdResult<Response> {
    let state: State = read_state(deps.storage)?;

    let quote_asset_amount = get_output_price_with_reserves(
        deps.as_ref(),
        &direction,
        base_asset_amount,
        state.quote_asset_reserve,
        state.base_asset_reserve,
    )?;

    // flip direction when updating reserve
    let mut update_direction = direction;
    if update_direction == Direction::AddToAmm {
        update_direction = Direction::RemoveFromAmm;
    } else {
        update_direction = Direction::AddToAmm;
    }

    update_reserve(
        deps.storage,
        env,
        update_direction,
        quote_asset_amount,
        base_asset_amount,
    )?;

    Ok(Response::new().add_attributes(vec![
        ("action", "swap_output"),
        ("input", &base_asset_amount.to_string()),
        ("output", &quote_asset_amount.to_string()),
    ]))
}

pub fn settle_funding(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let mut state: State = read_state(deps.storage)?;

    // check permission TODO add in the concept of counterparty
    if info.sender != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if env.block.time.seconds() < state.next_funding_time {
        return Err(StdError::generic_err("settle funding called too early"));
    }

    // twap price from oracle
    let underlying_price: Uint128 =
        query_underlying_twap_price(&deps.as_ref(), config.spot_price_twap_interval)?;

    // twap price from here, i.e. the amm
    let index_price: Uint128 =
        query_twap_price(deps.as_ref(), env.clone(), config.spot_price_twap_interval)?;

    let premium = calculate_premium(underlying_price, index_price)?;

    let premium_fraction = premium
        .value
        .checked_mul(Uint128::from(config.funding_period))?
        .checked_div(Uint128::from(ONE_DAY_IN_SECONDS))?;

    // update funding rate = premiumFraction / twapIndexPrice
    state.funding_rate = premium_fraction.checked_div(underlying_price)?;

    // in order to prevent multiple funding settlement during very short time after network congestion
    let min_next_funding_time = env.block.time.plus_seconds(config.funding_buffer_period);

    // floor((nextFundingTime + fundingPeriod) / 3600) * 3600
    let next_funding_time = (env.block.time.seconds() + config.funding_period)
        / ONE_HOUR_IN_SECONDS
        * ONE_HOUR_IN_SECONDS;

    // max(nextFundingTimeOnHourStart, minNextValidFundingTime)
    state.next_funding_time = if next_funding_time > min_next_funding_time.seconds() {
        next_funding_time
    } else {
        min_next_funding_time.seconds()
    };

    store_state(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        ("action", "settle_funding"),
        ("premium fraction", &premium_fraction.to_string()),
    ]))
}

pub fn calculate_premium(underlying: Uint128, index: Uint128) -> StdResult<PremiumResponse> {
    // if premium is positive: long pay short, amm get positive funding payment
    // if premium is negative: short pay long, amm get negative funding payment
    // if totalPositionSize.side * premiumFraction > 0, funding payment is positive which means profit
    let value: Uint128;
    let payer: LongShort;
    if index > underlying {
        value = index.checked_sub(underlying)?;
        payer = LongShort::Long;
    } else {
        value = underlying.checked_sub(index)?;
        payer = LongShort::Short;
    }

    Ok(PremiumResponse { value, payer })
}

pub fn get_input_price_with_reserves(
    deps: Deps,
    direction: &Direction,
    quote_asset_amount: Uint128,
    quote_asset_reserve: Uint128,
    base_asset_reserve: Uint128,
) -> StdResult<Uint128> {
    let config: Config = read_config(deps.storage)?;

    if quote_asset_amount == Uint128::zero() {
        Uint128::zero();
    }

    // k = x * y (divided by decimal places)
    let invariant_k = quote_asset_reserve
        .checked_mul(base_asset_reserve)?
        .checked_div(config.decimals)?;

    let quote_asset_after: Uint128 = match direction {
        Direction::AddToAmm => quote_asset_reserve.checked_add(quote_asset_amount)?,
        Direction::RemoveFromAmm => quote_asset_reserve.checked_sub(quote_asset_amount)?,
    };

    let base_asset_after: Uint128 = invariant_k
        .checked_mul(config.decimals)?
        .checked_div(quote_asset_after)?;

    let mut base_asset_bought = if base_asset_after > base_asset_reserve {
        base_asset_after - base_asset_reserve
    } else {
        base_asset_reserve - base_asset_after
    };

    let remainder = modulo(invariant_k, quote_asset_after);
    if remainder != Uint128::zero() {
        if *direction == Direction::AddToAmm {
            base_asset_bought = base_asset_bought.checked_sub(Uint128::new(1u128))?;
        } else {
            base_asset_bought = base_asset_bought.checked_add(Uint128::from(1u128))?;
        }
    }

    Ok(base_asset_bought)
}

pub fn get_output_price_with_reserves(
    deps: Deps,
    direction: &Direction,
    base_asset_amount: Uint128,
    quote_asset_reserve: Uint128,
    base_asset_reserve: Uint128,
) -> StdResult<Uint128> {
    let config: Config = read_config(deps.storage)?;

    if base_asset_amount == Uint128::zero() {
        Uint128::zero();
    }
    let invariant_k = quote_asset_reserve
        .checked_mul(base_asset_reserve)?
        .checked_div(config.decimals)?;

    let base_asset_after: Uint128 = match direction {
        Direction::AddToAmm => base_asset_reserve.checked_add(base_asset_amount)?,
        Direction::RemoveFromAmm => base_asset_reserve.checked_sub(base_asset_amount)?,
    };

    let quote_asset_after: Uint128 = invariant_k
        .checked_mul(config.decimals)?
        .checked_div(base_asset_after)?;

    let mut quote_asset_sold = if quote_asset_after > quote_asset_reserve {
        quote_asset_after - quote_asset_reserve
    } else {
        quote_asset_reserve - quote_asset_after
    };

    let remainder = modulo(invariant_k, base_asset_after);
    if remainder != Uint128::zero() {
        if *direction == Direction::AddToAmm {
            quote_asset_sold = quote_asset_sold.checked_sub(Uint128::from(1u128))?;
        } else {
            quote_asset_sold = quote_asset_sold.checked_add(Uint128::new(1u128))?;
        }
    }
    Ok(quote_asset_sold)
}

pub fn update_reserve(
    storage: &mut dyn Storage,
    env: Env,
    direction: Direction,
    quote_asset_amount: Uint128,
    base_asset_amount: Uint128,
) -> StdResult<Response> {
    let state: State = read_state(storage)?;
    let mut update_state = state.clone();

    match direction {
        Direction::AddToAmm => {
            update_state.quote_asset_reserve = update_state
                .quote_asset_reserve
                .checked_add(quote_asset_amount)?;
            update_state.base_asset_reserve =
                state.base_asset_reserve.checked_sub(base_asset_amount)?;

            // TODO think whether this is a very very bad idea or not
            update_state.total_position_size =
                state.total_position_size + Integer::from(base_asset_amount);
        }
        Direction::RemoveFromAmm => {
            update_state.base_asset_reserve = update_state
                .base_asset_reserve
                .checked_add(base_asset_amount)?;
            update_state.quote_asset_reserve =
                state.quote_asset_reserve.checked_sub(quote_asset_amount)?;
            update_state.total_position_size =
                state.total_position_size - Integer::from(base_asset_amount);
        }
    }

    store_state(storage, &update_state)?;

    add_reserve_snapshot(
        storage,
        env,
        update_state.quote_asset_reserve,
        update_state.base_asset_reserve,
    )?;

    Ok(Response::new().add_attributes(vec![("action", "update_reserve")]))
}

fn add_reserve_snapshot(
    storage: &mut dyn Storage,
    env: Env,
    quote_asset_reserve: Uint128,
    base_asset_reserve: Uint128,
) -> StdResult<Response> {
    let height = read_reserve_snapshot_counter(storage)?;
    let current_snapshot = read_reserve_snapshot(storage, height)?;

    if current_snapshot.block_height == env.block.height {
        let new_snapshot = ReserveSnapshot {
            quote_asset_reserve,
            base_asset_reserve,
            timestamp: current_snapshot.timestamp,
            block_height: current_snapshot.block_height,
        };

        update_reserve_snapshot(storage, &new_snapshot)?;
    } else {
        let new_snapshot = ReserveSnapshot {
            quote_asset_reserve,
            base_asset_reserve,
            timestamp: env.block.time,
            block_height: env.block.height,
        };

        store_reserve_snapshot(storage, &new_snapshot)?;
    }

    Ok(Response::default())
}
