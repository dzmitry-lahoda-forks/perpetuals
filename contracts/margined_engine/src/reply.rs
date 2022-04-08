use cosmwasm_std::{DepsMut, Env, Response, StdError, StdResult, SubMsg, Uint128};
use terraswap::asset::AssetInfo;

use crate::{
    handle::internal_increase_position,
    querier::query_vamm_state,
    state::{
        append_cumulative_premium_fraction, enter_restriction_mode, read_config, read_state,
        read_tmp_liquidator, read_tmp_swap, remove_tmp_liquidator, remove_tmp_swap, store_position,
        store_state, store_tmp_swap,
    },
    utils::{
        calc_remain_margin_with_funding_payment, clear_position, execute_transfer,
        execute_transfer_from, execute_transfer_to_insurance_fund, get_position, realize_bad_debt,
        side_to_direction, transfer_fees, update_open_interest_notional, withdraw,
    },
};

use margined_common::integer::Integer;
use margined_perp::margined_vamm::Direction;

// Increases position after successful execution of the swap
pub fn increase_position_reply(
    deps: DepsMut,
    env: Env,
    input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    println!("increase position reply");
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

    println!("margin: {}", position.margin);

    // TODO make my own decimal math lib
    let swap_margin = swap
        .open_notional
        .checked_mul(config.decimals)?
        .checked_div(swap.leverage)?;

    position.margin = position.margin.checked_add(swap_margin)?;
    println!("notional: {}", position.notional);
    println!("swap notional: {}", swap.open_notional);
    println!("leverage: {}", swap.leverage);
    println!("margin: {}", position.margin);

    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    let mut msgs: Vec<SubMsg> = vec![];

    // create transfer message
    if let AssetInfo::Token { .. } = config.eligible_collateral {
        msgs.push(
            execute_transfer_from(
                deps.storage,
                &swap.trader,
                &env.contract.address,
                swap_margin,
            )
            .unwrap(),
        );
    };

    println!("margin: {}", position.margin);

    // create messages to pay for toll and spread fees
    msgs.append(
        &mut transfer_fees(deps.as_ref(), swap.trader, swap.vamm, swap.open_notional).unwrap(),
    );
    println!("finish");
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
    let mut position = get_position(
        env,
        deps.storage,
        &swap.vamm,
        &swap.trader,
        swap.side.clone(),
    );

    update_open_interest_notional(
        &deps.as_ref(),
        &mut state,
        swap.vamm.clone(),
        Integer::new_negative(input),
    )?;

    let signed_output = if side_to_direction(swap.side) == Direction::AddToAmm {
        Integer::new_positive(output)
    } else {
        Integer::new_negative(output)
    };

    // now update the position
    position.size += signed_output;
    position.notional = position.notional.checked_sub(swap.open_notional)?;

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

    let msg: SubMsg;
    // now increase the position again if there is additional position
    let open_notional: Uint128;
    if swap.open_notional > output {
        open_notional = swap.open_notional.checked_sub(output)?;
        swap.open_notional = swap.open_notional.checked_sub(output)?;
    } else {
        open_notional = output.checked_sub(swap.open_notional)?;
        swap.open_notional = output.checked_sub(swap.open_notional)?;
    }
    if open_notional.checked_div(swap.leverage)? == Uint128::zero() {
        // create transfer message
        msg = execute_transfer(deps.storage, &swap.trader, margin_amount).unwrap();
        remove_tmp_swap(deps.storage);
    } else {
        store_tmp_swap(deps.storage, &swap)?;

        // TODO maybe we need to actually let the user define this limit
        msg = internal_increase_position(swap.vamm, swap.side, open_notional, Uint128::zero())?
    }

    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_submessage(msg)
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

    let remain_margin =
        calc_remain_margin_with_funding_payment(deps.as_ref(), position.clone(), margin_delta)?;

    let mut messages: Vec<SubMsg> = vec![];

    if !remain_margin.bad_debt.is_zero() {
        realize_bad_debt(
            deps.storage,
            env.contract.address.clone(),
            remain_margin.bad_debt,
            &mut messages,
        )?;
    }
    if !remain_margin.margin.is_zero() {
        let withdraw_messages = withdraw(
            deps.as_ref(),
            env.clone(),
            &mut state,
            &swap.trader,
            &config.insurance_fund,
            config.eligible_collateral,
            remain_margin.margin,
        )
        .unwrap();

        for message in withdraw_messages.iter() {
            messages.push(message.clone());
        }
    }

    // now start putting the response together
    let mut response = Response::new();
    response = response.add_submessages(messages.clone());

    // create messages to pay for toll and spread fees
    let fee_msgs = transfer_fees(
        deps.as_ref(),
        swap.trader,
        swap.vamm.clone(),
        position.notional,
    )
    .unwrap();
    response = response.add_submessages(fee_msgs);

    let value = margin_delta
        + Integer::new_positive(remain_margin.bad_debt)
        + Integer::new_positive(position.notional);

    update_open_interest_notional(
        &deps.as_ref(),
        &mut state,
        swap.vamm,
        value.invert_sign(),
        // Integer::new_negative(output),
    )?;

    position = clear_position(env, position)?;

    // remove_position(deps.storage, &position)?;
    store_position(deps.storage, &position)?;
    store_state(deps.storage, &state)?;

    remove_tmp_swap(deps.storage);
    Ok(response.add_attributes(vec![
        ("action", "close_position_reply"),
        (
            "funding_payment",
            &remain_margin.funding_payment.to_string(),
        ),
        ("bad_debt", &remain_margin.bad_debt.to_string()),
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

    let mut messages: Vec<SubMsg> = vec![];

    if !remain_margin.bad_debt.is_zero() {
        realize_bad_debt(
            deps.storage,
            env.contract.address.clone(),
            remain_margin.bad_debt,
            &mut messages,
        )?;
    }

    let fee_to_insurance = if !remain_margin.margin.is_zero() {
        remain_margin.margin
    } else {
        Uint128::zero()
    };

    if !fee_to_insurance.is_zero() {
        messages.push(
            execute_transfer(deps.storage, &config.insurance_fund, fee_to_insurance).unwrap(),
        );
    }

    // pay liquidation fees
    let liquidator = liquidator.unwrap();

    // calculate token balance that should be remaining once
    // insurance fees have been paid
    let withdraw_messages = withdraw(
        deps.as_ref(),
        env.clone(),
        &mut state,
        &liquidator,
        &config.insurance_fund,
        config.eligible_collateral,
        liquidation_fee,
    )
    .unwrap();

    for message in withdraw_messages.iter() {
        messages.push(message.clone());
    }

    position = clear_position(env.clone(), position)?;

    store_position(deps.storage, &position)?;

    remove_tmp_swap(deps.storage);
    remove_tmp_liquidator(deps.storage);

    enter_restriction_mode(deps.storage, swap.vamm, env.block.height)?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![
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

    // pay liquidation fees
    let liquidator = liquidator.unwrap();

    // calculate token balance that should be remaining once
    // insurance fees have been paid
    let withdraw_messages = withdraw(
        deps.as_ref(),
        env.clone(),
        &mut state,
        &liquidator,
        &config.insurance_fund,
        config.eligible_collateral,
        liquidation_fee,
    )
    .unwrap();

    for message in withdraw_messages.iter() {
        messages.push(message.clone());
    }

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
