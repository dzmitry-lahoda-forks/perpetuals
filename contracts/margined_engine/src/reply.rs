use cosmwasm_std::{DepsMut, Env, Response, StdError, StdResult, SubMsg, Uint128};
use std::cmp::Ordering;
use terraswap::asset::AssetInfo;

use crate::{
    handle::internal_increase_position,
    messages::{
        execute_transfer, execute_transfer_from, execute_transfer_to_insurance_fund, transfer_fees,
        withdraw,
    },
    querier::query_vamm_state,
    state::{
        append_cumulative_premium_fraction, enter_restriction_mode, read_config, read_state,
        read_tmp_liquidator, read_tmp_swap, remove_tmp_liquidator, remove_tmp_swap, store_position,
        store_state, store_tmp_swap,
    },
    utils::{
        calc_remain_margin_with_funding_payment, clear_position, get_position, realize_bad_debt,
        side_to_direction, update_open_interest_notional,
    },
};

use margined_common::integer::Integer;
use margined_perp::{margined_engine::RemainMarginResponse, margined_vamm::Direction};

// Increases position after successful execution of the swap
pub fn increase_position_reply(
    deps: DepsMut,
    env: Env,
    input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    let tmp_swap = read_tmp_swap(deps.storage)?;
    if tmp_swap.is_none() {
        return Err(StdError::generic_err("no temporary position"));
    }

    let mut swap = tmp_swap.unwrap();
    let mut position = get_position(
        env.clone(),
        deps.storage,
        &swap.vamm,
        &swap.trader,
        swap.side.clone(),
    );

    let direction = side_to_direction(swap.side);

    let signed_output = if direction == Direction::AddToAmm {
        Integer::new_positive(output)
    } else {
        Integer::new_negative(output)
    };

    update_open_interest_notional(
        &deps.as_ref(),
        &mut state,
        swap.vamm.clone(),
        Integer::new_positive(input),
    )?;

    // now update the position
    position.size += signed_output;
    position.notional = position.notional.checked_add(swap.open_notional)?;
    position.direction = direction;

    // TODO make my own decimal math lib
    let swap_margin = swap
        .open_notional
        .checked_mul(config.decimals)?
        .checked_div(swap.leverage)?;

    swap.margin_to_vault = swap
        .margin_to_vault
        .checked_add(Integer::new_positive(swap_margin))?;

    let RemainMarginResponse {
        funding_payment: _,
        margin,
        bad_debt: _,
        latest_premium_fraction: _,
    } = calc_remain_margin_with_funding_payment(
        deps.as_ref(),
        position.clone(),
        Integer::new_positive(swap_margin),
    )?;

    position.margin = margin;

    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    let mut msgs: Vec<SubMsg> = vec![];

    // create transfer messages TODO make this a nice function for use in each
    match swap.margin_to_vault.cmp(&Integer::zero()) {
        Ordering::Less => {
            msgs.append(
                &mut withdraw(
                    deps.as_ref(),
                    env,
                    &mut state,
                    &swap.trader,
                    &config.insurance_fund,
                    config.eligible_collateral,
                    swap.margin_to_vault.value,
                )
                .unwrap(),
            );
        }
        Ordering::Greater => {
            if let AssetInfo::Token { .. } = config.eligible_collateral {
                msgs.push(
                    execute_transfer_from(
                        deps.storage,
                        &swap.trader,
                        &env.contract.address,
                        swap.margin_to_vault.value,
                    )
                    .unwrap(),
                );
            };
        }
        _ => {}
    }

    // create messages to pay for toll and spread fees, check flag is true if this follows a reverse
    if !swap.fees_paid {
        msgs.append(
            &mut transfer_fees(deps.as_ref(), swap.trader, swap.vamm, swap.open_notional).unwrap(),
        )
    };

    remove_tmp_swap(deps.storage);
    Ok(Response::new()
        .add_submessages(msgs)
        .add_attributes(vec![("action", "increase_position")]))
}

// Decreases position after successful execution of the swap
pub fn decrease_position_reply(
    deps: DepsMut,
    env: Env,
    input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let mut state = read_state(deps.storage)?;
    let tmp_swap = read_tmp_swap(deps.storage)?;
    if tmp_swap.is_none() {
        return Err(StdError::generic_err("no temporary position"));
    }

    let swap = tmp_swap.unwrap();
    update_open_interest_notional(
        &deps.as_ref(),
        &mut state,
        swap.vamm.clone(),
        Integer::new_negative(input),
    )?;

    let signed_output = if side_to_direction(swap.side.clone()) == Direction::AddToAmm {
        Integer::new_positive(output)
    } else {
        Integer::new_negative(output)
    };

    let mut position = get_position(
        env,
        deps.storage,
        &swap.vamm,
        &swap.trader,
        swap.side.clone(),
    );

    // realized_pnl = unrealized_pnl * close_ratio
    let realized_pnl = if !position.size.is_zero() {
        swap.unrealized_pnl.checked_mul(signed_output.abs())? / position.size.abs()
    } else {
        Integer::zero()
    };

    let RemainMarginResponse {
        funding_payment: _,
        margin,
        bad_debt: _,
        latest_premium_fraction,
    } = calc_remain_margin_with_funding_payment(deps.as_ref(), position.clone(), realized_pnl)?;

    let unrealized_pnl_after = swap.unrealized_pnl - realized_pnl;

    let remaining_notional = if position.size > Integer::zero() {
        Integer::new_positive(swap.position_notional)
            - Integer::new_positive(swap.open_notional)
            - unrealized_pnl_after
    } else {
        unrealized_pnl_after + Integer::new_positive(swap.position_notional)
            - Integer::new_positive(swap.open_notional)
    };

    // now update the position
    position.size += signed_output;
    position.notional = remaining_notional.value;
    position.margin = margin;
    position.last_updated_premium_fraction = latest_premium_fraction;

    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    // remove the tmp position
    remove_tmp_swap(deps.storage);

    Ok(Response::new().add_attributes(vec![("action", "decrease_position")]))
}

// reverse position after successful execution of the swap
pub fn reverse_position_reply(
    deps: DepsMut,
    env: Env,
    _input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let mut state = read_state(deps.storage)?;
    let tmp_swap = read_tmp_swap(deps.storage)?;
    if tmp_swap.is_none() {
        return Err(StdError::generic_err("no temporary position"));
    }

    let mut swap = tmp_swap.unwrap();
    let mut position = get_position(
        env.clone(),
        deps.storage,
        &swap.vamm,
        &swap.trader,
        swap.side.clone(),
    );

    update_open_interest_notional(
        &deps.as_ref(),
        &mut state,
        swap.vamm.clone(),
        Integer::new_negative(output),
    )?;

    let margin_amount = position.margin;

    position = clear_position(env, position)?;

    // now increase the position again if there is additional position
    let current_open_notional = swap.open_notional;
    swap.open_notional = if swap.open_notional > output {
        swap.open_notional.checked_sub(output)?
    } else {
        output.checked_sub(swap.open_notional)?
    };

    let mut msgs: Vec<SubMsg> = vec![];
    if swap.open_notional.checked_div(swap.leverage)? == Uint128::zero() {
        // create transfer message
        msgs.push(execute_transfer(deps.storage, &swap.trader, margin_amount).unwrap());

        remove_tmp_swap(deps.storage);
    } else {
        swap.margin_to_vault =
            Integer::new_negative(margin_amount).checked_sub(swap.unrealized_pnl)?;
        swap.unrealized_pnl = Integer::zero();
        swap.fees_paid = true;

        msgs.push(internal_increase_position(
            swap.vamm.clone(),
            swap.side.clone(),
            swap.open_notional,
            Uint128::zero(),
        )?);

        store_tmp_swap(deps.storage, &swap)?;
    }

    msgs.append(
        &mut transfer_fees(deps.as_ref(), swap.trader, swap.vamm, current_open_notional).unwrap(),
    );

    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_submessages(msgs)
        .add_attributes(vec![("action", "reverse_position")]))
}

// Closes position after successful execution of the swap
pub fn close_position_reply(
    deps: DepsMut,
    env: Env,
    _input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    let tmp_swap = read_tmp_swap(deps.storage)?;
    if tmp_swap.is_none() {
        return Err(StdError::generic_err("no temporary position"));
    }

    let swap = tmp_swap.unwrap();
    let mut position = get_position(
        env.clone(),
        deps.storage,
        &swap.vamm.clone(),
        &swap.trader,
        swap.side.clone(),
    );

    let margin_delta = if position.direction != Direction::AddToAmm {
        Integer::new_positive(swap.open_notional) - Integer::new_positive(output)
    } else {
        Integer::new_positive(output) - Integer::new_positive(swap.open_notional)
    };

    let RemainMarginResponse {
        funding_payment,
        margin,
        bad_debt,
        latest_premium_fraction: _,
    } = calc_remain_margin_with_funding_payment(deps.as_ref(), position.clone(), margin_delta)?;

    let mut msgs: Vec<SubMsg> = vec![];

    if !bad_debt.is_zero() {
        realize_bad_debt(
            deps.storage,
            env.contract.address.clone(),
            bad_debt,
            &mut msgs,
        )?;
    }

    if !margin.is_zero() {
        msgs.append(
            &mut withdraw(
                deps.as_ref(),
                env.clone(),
                &mut state,
                &swap.trader,
                &config.insurance_fund,
                config.eligible_collateral,
                margin,
            )
            .unwrap(),
        );
    }

    msgs.append(
        &mut transfer_fees(
            deps.as_ref(),
            swap.trader,
            swap.vamm.clone(),
            position.notional,
        )
        .unwrap(),
    );

    let value =
        margin_delta + Integer::new_positive(bad_debt) + Integer::new_positive(position.notional);

    update_open_interest_notional(&deps.as_ref(), &mut state, swap.vamm, value.invert_sign())?;

    position = clear_position(env, position)?;

    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    remove_tmp_swap(deps.storage);
    Ok(Response::new().add_submessages(msgs).add_attributes(vec![
        ("action", "close_position_reply"),
        ("funding_payment", &funding_payment.to_string()),
        ("bad_debt", &bad_debt.to_string()),
    ]))
}

// Liquidates position after successful execution of the swap
pub fn liquidate_reply(
    deps: DepsMut,
    env: Env,
    _input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    let tmp_swap = read_tmp_swap(deps.storage)?;
    if tmp_swap.is_none() {
        return Err(StdError::generic_err("no temporary position"));
    }

    let liquidator = read_tmp_liquidator(deps.storage)?;
    if liquidator.is_none() {
        return Err(StdError::generic_err("no liquidator"));
    }

    let swap = tmp_swap.unwrap();
    let mut position = get_position(
        env.clone(),
        deps.storage,
        &swap.vamm,
        &swap.trader,
        swap.side.clone(),
    );

    // calculate delta from trade and whether it was profitable or a loss
    let margin_delta = if position.direction != Direction::AddToAmm {
        Integer::new_positive(swap.open_notional) - Integer::new_positive(output)
    } else {
        Integer::new_positive(output) - Integer::new_positive(swap.open_notional)
    };

    let mut remain_margin =
        calc_remain_margin_with_funding_payment(deps.as_ref(), position.clone(), margin_delta)?;

    // calculate liquidation penalty and fee for liquidator
    let liquidation_penalty: Uint128 = output
        .checked_mul(config.liquidation_fee)?
        .checked_div(config.decimals)?;

    let liquidation_fee: Uint128 = liquidation_penalty.checked_div(Uint128::from(2u64))?;

    if liquidation_fee > remain_margin.margin {
        let bad_debt = liquidation_fee.checked_sub(remain_margin.margin)?;
        remain_margin.bad_debt = remain_margin.bad_debt.checked_add(bad_debt)?;
    } else {
        remain_margin.margin = remain_margin.margin.checked_sub(liquidation_fee)?;
    }

    let mut msgs: Vec<SubMsg> = vec![];

    if !remain_margin.bad_debt.is_zero() {
        realize_bad_debt(
            deps.storage,
            env.contract.address.clone(),
            remain_margin.bad_debt,
            &mut msgs,
        )?;
    }

    let fee_to_insurance = if !remain_margin.margin.is_zero() {
        remain_margin.margin
    } else {
        Uint128::zero()
    };

    if !fee_to_insurance.is_zero() {
        msgs.push(
            execute_transfer(deps.storage, &config.insurance_fund, fee_to_insurance).unwrap(),
        );
    }

    // pay liquidation fees
    let liquidator = liquidator.unwrap();

    // calculate token balance that should be remaining once
    // insurance fees have been paid
    msgs.append(
        &mut withdraw(
            deps.as_ref(),
            env.clone(),
            &mut state,
            &liquidator,
            &config.insurance_fund,
            config.eligible_collateral,
            liquidation_fee,
        )
        .unwrap(),
    );

    position = clear_position(env.clone(), position)?;

    store_position(deps.storage, &position)?;

    remove_tmp_swap(deps.storage);
    remove_tmp_liquidator(deps.storage);

    enter_restriction_mode(deps.storage, swap.vamm, env.block.height)?;

    Ok(Response::new().add_submessages(msgs).add_attributes(vec![
        ("action", "liquidate_reply"),
        ("liquidation_fee", &liquidation_fee.to_string()),
        ("pnl", &margin_delta.to_string()),
    ]))
}

// Partially liquidates the position
pub fn partial_liquidation_reply(
    deps: DepsMut,
    env: Env,
    input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    let tmp_swap = read_tmp_swap(deps.storage)?;
    if tmp_swap.is_none() {
        return Err(StdError::generic_err("no temporary position"));
    }

    let liquidator = read_tmp_liquidator(deps.storage)?;
    if liquidator.is_none() {
        return Err(StdError::generic_err("no liquidator"));
    }

    let swap = tmp_swap.unwrap();
    let mut position = get_position(
        env.clone(),
        deps.storage,
        &swap.vamm,
        &swap.trader,
        swap.side.clone(),
    );

    // calculate delta from trade and whether it was profitable or a loss
    let realized_pnl = (swap.unrealized_pnl
        * Integer::new_positive(config.partial_liquidation_margin_ratio))
        / Integer::new_positive(config.decimals);

    let liquidation_penalty: Uint128 = output
        .checked_mul(config.liquidation_fee)?
        .checked_div(config.decimals)?;

    let liquidation_fee: Uint128 = liquidation_penalty.checked_div(Uint128::from(2u64))?;

    let signed_input = if position.size < Integer::zero() {
        Integer::new_positive(input)
    } else {
        Integer::new_negative(input)
    };

    position.size += signed_input;

    position.margin = position
        .margin
        .checked_sub(realized_pnl.value)?
        .checked_sub(liquidation_penalty)?;

    // calculate openNotional (it's different depends on long or short side)
    // long: unrealizedPnl = positionNotional - openNotional => openNotional = positionNotional - unrealizedPnl
    // short: unrealizedPnl = openNotional - positionNotional => openNotional = positionNotional + unrealizedPnl
    // positionNotional = oldPositionNotional - exchangedQuoteAssetAmount
    position.notional = if position.size.is_positive() {
        position
            .notional
            .checked_sub(swap.open_notional)?
            .checked_sub(realized_pnl.value)?
    } else {
        realized_pnl
            .value
            .checked_add(position.notional)?
            .checked_sub(swap.open_notional)?
    };

    let mut messages: Vec<SubMsg> = vec![];
    if !liquidation_fee.is_zero() {
        messages
            .push(execute_transfer(deps.storage, &config.insurance_fund, liquidation_fee).unwrap());
    }

    // calculate token balance that should be remaining once
    // insurance fees have been paid
    messages.append(
        &mut withdraw(
            deps.as_ref(),
            env.clone(),
            &mut state,
            &liquidator.unwrap(),
            &config.insurance_fund,
            config.eligible_collateral,
            liquidation_fee,
        )
        .unwrap(),
    );

    store_position(deps.storage, &position)?;

    remove_tmp_swap(deps.storage);
    remove_tmp_liquidator(deps.storage);

    enter_restriction_mode(deps.storage, swap.vamm, env.block.height)?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![
            ("action", "partial_liquidate_reply"),
            ("liquidation_fee", &liquidation_fee.to_string()),
            ("pnl", &realized_pnl.to_string()),
        ]))
}

/// pays funding, if funding rate is positive, traders with long position
/// pay traders with short position and vice versa.
pub fn pay_funding_reply(
    deps: DepsMut,
    env: Env,
    premium_fraction: Integer,
    sender: String,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let vamm = deps.api.addr_validate(&sender)?;

    // update the cumulative premium fraction
    append_cumulative_premium_fraction(deps.storage, vamm.clone(), premium_fraction)?;

    let total_position_size =
        query_vamm_state(&deps.as_ref(), vamm.to_string())?.total_position_size;

    let funding_payment =
        total_position_size * premium_fraction / Integer::new_positive(config.decimals);

    let msg: SubMsg = if funding_payment.is_negative() {
        execute_transfer_from(
            deps.storage,
            &config.insurance_fund,
            &env.contract.address,
            funding_payment.value,
        )?
    } else {
        execute_transfer_to_insurance_fund(deps.as_ref(), env, funding_payment.value)?
    };

    Ok(Response::new()
        .add_submessage(msg)
        .add_attributes(vec![("action", "pay_funding_reply")]))
}
