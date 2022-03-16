use cosmwasm_std::{DepsMut, Env, Response, StdError, StdResult, SubMsg, Uint128};

use crate::{
    handle::{
        calc_pnl, calc_remain_margin_with_funding_payment, clear_position,
        get_position_notional_unrealized_pnl, internal_increase_position,
    },
    state::{
        read_config, read_state, read_tmp_liquidator, read_tmp_swap, remove_tmp_liquidator,
        remove_tmp_swap, store_position, store_state, store_tmp_swap,
    },
    utils::{
        execute_transfer, execute_transfer_from, get_position, realize_bad_debt, side_to_direction,
        transfer_fee,
    },
};

use margined_perp::margined_engine::{Pnl, PnlCalcOption};
use margined_perp::querier::query_token_balance;

// Increases position after successful execution of the swap
pub fn increase_position_reply(
    deps: DepsMut,
    env: Env,
    _input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    println!("HERE?");
    let config = read_config(deps.storage)?;
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

    // now update the position
    position.size = position.size.checked_add(output)?;
    position.notional = position.notional.checked_add(swap.open_notional)?;
    position.direction = side_to_direction(swap.side);

    // TODO make my own decimal math lib
    position.margin = position
        .notional
        .checked_mul(config.decimals)?
        .checked_div(swap.leverage)?;

    store_position(deps.storage, &position)?;

    // create transfer message
    let msg = execute_transfer_from(
        deps.storage,
        &swap.trader,
        &env.contract.address,
        position.margin,
    )
    .unwrap();

    // create messages to pay for toll and spread fees
    let fee_msgs = transfer_fee(deps.as_ref(), swap.trader, swap.vamm, position.notional).unwrap();

    remove_tmp_swap(deps.storage);
    Ok(Response::new()
        .add_submessage(msg)
        .add_messages(fee_msgs)
        .add_attributes(vec![("action", "increase_position")]))
}

// Decreases position after successful execution of the swap
pub fn decrease_position_reply(
    deps: DepsMut,
    env: Env,
    _input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
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

    // now update the position
    position.size = position.size.checked_sub(output)?;
    position.notional = position.notional.checked_sub(swap.open_notional)?;

    store_position(deps.storage, &position)?;

    // remove the tmp position
    remove_tmp_swap(deps.storage);

    Ok(Response::new().add_attributes(vec![("action", "decrease_position")]))
}

// Decreases position after successful execution of the swap
pub fn reverse_position_reply(
    deps: DepsMut,
    env: Env,
    _input: Uint128,
    output: Uint128,
) -> StdResult<Response> {
    let response: Response = Response::new();
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

        msg = internal_increase_position(swap.vamm, swap.side, open_notional)?
    }

    store_position(deps.storage, &position)?;

    Ok(response
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

    let pnl = calc_pnl(output, swap.open_notional, position.direction.clone())?;

    let remain_margin = calc_remain_margin_with_funding_payment(&position, pnl.clone())?;

    let mut messages: Vec<SubMsg> = vec![];

    // TODO Make this less ugly
    if pnl.profit_loss == Pnl::Profit {
        let token_balance = query_token_balance(
            deps.as_ref(),
            config.eligible_collateral,
            env.contract.address.clone(),
        )?;
        if remain_margin.margin <= token_balance {
            messages
                .push(execute_transfer(deps.storage, &swap.trader, remain_margin.margin).unwrap());
        } else {
            let short_fall = remain_margin.margin.checked_sub(token_balance)?;

            let mut state = read_state(deps.storage)?;

            messages.push(execute_transfer(deps.storage, &swap.trader, token_balance).unwrap());
            messages.push(
                execute_transfer_from(
                    deps.storage,
                    &config.insurance_fund,
                    &swap.trader,
                    short_fall,
                )
                .unwrap(),
            );
            state.bad_debt = short_fall;
            store_state(deps.storage, &state)?;
        }
    } else if pnl.value < position.margin {
        // create transfer message
        messages.push(execute_transfer(deps.storage, &swap.trader, remain_margin.margin).unwrap());
    } else {
        realize_bad_debt(
            deps.storage,
            env.contract.address.clone(),
            remain_margin.bad_debt,
            &mut messages,
        )?;
    };

    // now start putting the response together
    let mut response = Response::new();
    response = response.add_submessages(messages);

    // create messages to pay for toll and spread fees
    let fee_msgs = transfer_fee(deps.as_ref(), swap.trader, swap.vamm, position.notional).unwrap();
    response = response.add_messages(fee_msgs);

    position = clear_position(env, position)?;

    // remove_position(deps.storage, &position)?;
    store_position(deps.storage, &position)?;

    remove_tmp_swap(deps.storage);

    Ok(response)
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
    let pnl = calc_pnl(output, swap.open_notional, position.direction.clone())?;

    let mut remain_margin = calc_remain_margin_with_funding_payment(&position, pnl.clone())?;

    let liquidation_fee: Uint128 = output
        .checked_mul(config.liquidation_fee)?
        .checked_div(config.decimals)?
        .checked_div(Uint128::from(2u64))?;
    if liquidation_fee > remain_margin.margin {
        let bad_debt = liquidation_fee.checked_sub(remain_margin.margin)?;
        remain_margin.bad_debt = remain_margin.bad_debt.checked_add(bad_debt)?;
    } else {
        remain_margin.margin = remain_margin.margin.checked_sub(liquidation_fee)?;
    }

    let mut messages: Vec<SubMsg> = vec![];
    if remain_margin.bad_debt > Uint128::zero() {
        realize_bad_debt(
            deps.storage,
            env.contract.address.clone(),
            remain_margin.bad_debt,
            &mut messages,
        )?;
    }
    let mut fee_to_insurance = Uint128::zero();
    if !remain_margin.margin.is_zero() {
        fee_to_insurance = remain_margin.margin;
    }

    if !fee_to_insurance.is_zero() {
        messages.push(
            execute_transfer(deps.storage, &config.insurance_fund, fee_to_insurance).unwrap(),
        );
    }

    // pay liquidation fees
    let liquidator = liquidator.unwrap();

    // calculate token balance that should be remaining once
    // insurance fees have been paid
    let token_balance = query_token_balance(
        deps.as_ref(),
        config.eligible_collateral,
        env.contract.address.clone(),
    )?
    .checked_sub(fee_to_insurance)?;

    if token_balance < liquidation_fee {
        let short_fall = liquidation_fee.checked_sub(token_balance)?;

        if !token_balance.is_zero() {
            messages.push(execute_transfer(deps.storage, &liquidator, token_balance).unwrap());
        }
        messages.push(
            execute_transfer_from(
                deps.storage,
                &config.insurance_fund,
                &liquidator,
                short_fall,
            )
            .unwrap(),
        );
        state.bad_debt = short_fall;

        store_state(deps.storage, &state)?;
    } else {
        messages.push(execute_transfer(deps.storage, &liquidator, liquidation_fee).unwrap());
    }

    position = clear_position(env, position)?;

    // remove_position(deps.storage, &position)?;
    store_position(deps.storage, &position)?;

    remove_tmp_swap(deps.storage);
    remove_tmp_liquidator(deps.storage);
    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![
            ("action", "liquidate_reply"),
            ("liquidation_fee", &liquidation_fee.to_string()),
            ("pnl", &pnl.value.to_string()),
        ]))
}
