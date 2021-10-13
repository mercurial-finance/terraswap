use crate::error::ContractError;
use crate::response::MsgInstantiateContractResponse;
use crate::state::PAIR_INFO;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use crate::curve::Curve;

use cosmwasm_bignumber::{Decimal256, Uint256};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use integer_sqrt::IntegerSquareRoot;
use protobuf::Message;
use std::str::FromStr;
use terraswap::asset::{Asset, AssetInfo, PairInfo, PairInfoRaw};
use terraswap::pair::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolResponse, QueryMsg,
    ReverseSimulationResponse, SimulationResponse,
};
use terraswap::querier::query_supply;
use terraswap::token::InstantiateMsg as TokenInstantiateMsg;

const INSTANTIATE_REPLY_ID: u64 = 1;

/// Commission rate == 0.3%
// const AMPLIFICATION: u64 = 60;
// const FEE_NUMERATOR: u64 = 4;
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let mut asset_infos = vec![];
    for asset in msg.asset_infos.iter() {
        asset_infos.push(asset.to_raw(deps.api)?);
    }
    let pair_info: &PairInfoRaw = &PairInfoRaw {
        contract_addr: deps.api.addr_canonicalize(env.contract.address.as_str())?,
        liquidity_token: CanonicalAddr::from(vec![]),
        asset_infos: asset_infos,
        amplification: msg.amplification,
        fee: msg.fee,
    };

    PAIR_INFO.save(deps.storage, pair_info)?;

    Ok(Response::new().add_submessage(SubMsg {
        // Create LP token
        msg: WasmMsg::Instantiate {
            admin: None,
            code_id: msg.token_code_id,
            msg: to_binary(&TokenInstantiateMsg {
                name: "terraswap liquidity token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
            })?,
            funds: vec![],
            label: "".to_string(),
        }
        .into(),
        gas_limit: None,
        id: INSTANTIATE_REPLY_ID,
        reply_on: ReplyOn::Success,
    }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        // ExecuteMsg::WithdrawSingleLiquidity {
        //     asset,
        //     unmint_amount,
        //     min_out_amount,
        // } => withdraw_single_liquidity(deps, env, info, asset, unmint_amount, min_out_amount),
        ExecuteMsg::ProvideLiquidity {
            assets,
            min_out_amount,
            receiver,
        } => provide_liquidity(deps, env, info, assets, min_out_amount, receiver),
        ExecuteMsg::Swap {
            offer_asset,
            ask_asset,
            min_out_amount,
            to,
        } => {
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(deps.api.addr_validate(&to_addr)?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info.clone(),
                info.sender,
                offer_asset,
                ask_asset,
                min_out_amount,
                to_addr,
            )
        }
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Swap {
            ask_asset,
            min_out_amount,
            to,
        }) => {
            // only asset contract can execute this message
            let mut authorized: bool = false;
            let config: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
            let pools: Vec<Asset> =
                config.query_pools(&deps.querier, deps.api, env.contract.address.clone())?;
            for pool in pools.iter() {
                if let AssetInfo::Token { contract_addr, .. } = &pool.info {
                    if contract_addr == &info.sender {
                        authorized = true;
                    }
                }
            }

            if !authorized {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(deps.api.addr_validate(to_addr.as_str())?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info,
                Addr::unchecked(cw20_msg.sender),
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: contract_addr.to_string(),
                    },
                    amount: cw20_msg.amount,
                },
                ask_asset,
                min_out_amount,
                to_addr,
            )
        }
        Ok(Cw20HookMsg::WithdrawSingleLiquidity {
            asset,
            min_out_amount,
        }) => {
            let config: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
            if deps.api.addr_canonicalize(info.sender.as_str())? != config.liquidity_token {
                return Err(ContractError::Unauthorized {});
            }
            withdraw_single_liquidity(deps, env, info, asset, cw20_msg.amount, min_out_amount)
        }
        Ok(Cw20HookMsg::WithdrawLiquidity {}) => {
            let config: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
            if deps.api.addr_canonicalize(info.sender.as_str())? != config.liquidity_token {
                return Err(ContractError::Unauthorized {});
            }

            let sender_addr = deps.api.addr_validate(cw20_msg.sender.as_str())?;
            withdraw_liquidity(deps, env, info, sender_addr, cw20_msg.amount)
        }
        Err(err) => Err(ContractError::Std(err)),
    }
}

/// This just stores the result for future query
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;
    let liquidity_token = res.get_contract_address();

    let api = deps.api;
    PAIR_INFO.update(deps.storage, |mut meta| -> StdResult<_> {
        meta.liquidity_token = api.addr_canonicalize(liquidity_token)?;
        Ok(meta)
    })?;

    Ok(Response::new().add_attribute("liquidity_token_addr", liquidity_token))
}

/// CONTRACT - should approve contract to use the amount of token
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    min_out_amount: Uint128,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    for asset in assets.iter() {
        asset.assert_sent_native_token_balance(&info)?;
    }

    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let mut pools: Vec<Asset> =
        pair_info.query_pools(&deps.querier, deps.api, env.contract.address.clone())?;

    let mut deposits = Vec::with_capacity(pools.len());
    for (i, asset) in assets.iter().enumerate() {
        assert_eq!(asset.info, pools[i].info);
        deposits.push(asset.amount);
    }
    // let deposits: [Uint128; 2] = [
    //     assets
    //         .iter()
    //         .find(|a| a.info.equal(&pools[0].info))
    //         .map(|a| a.amount)
    //         .expect("Wrong asset info is given"),
    //     assets
    //         .iter()
    //         .find(|a| a.info.equal(&pools[1].info))
    //         .map(|a| a.amount)
    //         .expect("Wrong asset info is given"),
    // ];

    let mut messages: Vec<CosmosMsg> = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the pool is token contract, then we need to execute TransferFrom msg to receive funds
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: deposits[i],
                })?,
                funds: vec![],
            }));
        } else {
            // If the asset is native token, balance is already increased
            // To calculated properly we should subtract user deposit from the pool
            pool.amount = pool.amount.checked_sub(deposits[i])?;
        }
    }

    // assert slippage tolerance
    // assert_slippage_tolerance(&slippage_tolerance, &deposits, &pools)?;

    let liquidity_token = deps.api.addr_humanize(&pair_info.liquidity_token)?;
    let total_share = query_supply(&deps.querier, liquidity_token)?;

    let mut old_balances = Vec::with_capacity(pools.len());
    let mut new_balances = Vec::with_capacity(pools.len());
    for (i, pool) in pools.iter().enumerate() {
        let pool_balance = pool.amount.u128();
        old_balances.push(pool_balance);
        new_balances.push(pool_balance.checked_add(deposits[i].u128()).unwrap());
    }

    // TODO find better way for type conversion
    let mint_amount = Curve {
        amp: pair_info.amplification.u128() as u64,
        fee_numerator: pair_info.fee.u128() as u64,
    }
    .deposit(&old_balances, &new_balances, total_share.u128() as u64)
    .unwrap();

    if mint_amount < min_out_amount.u128() as u64 {
        return Err(ContractError::MaxSlippageAssertion {});
    }

    let share = Uint128::from(mint_amount as u128);

    // prevent providing free token
    if share.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // mint LP token to sender
    let receiver = receiver.unwrap_or_else(|| info.sender.to_string());
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps
            .api
            .addr_humanize(&pair_info.liquidity_token)?
            .to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Mint {
            recipient: receiver.to_string(),
            amount: share,
        })?,
        funds: vec![],
    }));

    // let mut assets_msg = "";
    // for asset in assets.iter() {
    //     assets_msg = sformat!("{} {}", assets_msg, asset);
    // }
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "provide_liquidity"),
        ("sender", info.sender.as_str()),
        ("receiver", receiver.as_str()),
        // ("assets", assets_msg),
        ("share", &share.to_string()),
    ]))
}

pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let liquidity_addr: Addr = deps.api.addr_humanize(&pair_info.liquidity_token)?;

    let pools: Vec<Asset> = pair_info.query_pools(&deps.querier, deps.api, env.contract.address)?;
    let total_share: Uint128 = query_supply(&deps.querier, liquidity_addr)?;

    let share_ratio: Decimal = Decimal::from_ratio(amount, total_share);
    let refund_assets: Vec<Asset> = pools
        .iter()
        .map(|a| Asset {
            info: a.info.clone(),
            amount: a.amount * share_ratio,
        })
        .collect();

    let mut refund_assets_msg: Vec<CosmosMsg> = refund_assets
        .iter()
        .map(|a| a.clone().into_msg(&deps.querier, sender.clone()).unwrap())
        .collect();

    refund_assets_msg.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps
            .api
            .addr_humanize(&pair_info.liquidity_token)?
            .to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
        funds: vec![],
    }));

    // let mut assets_msg = "";
    // for asset in pools.iter() {
    //     assets_msg = &format!("{} {}", assets_msg, asset);
    // }
    // update pool info
    Ok(Response::new()
        .add_messages(refund_assets_msg)
        .add_attributes(vec![
            ("action", "withdraw_liquidity"),
            ("sender", sender.as_str()),
            ("withdrawn_share", &amount.to_string()),
            // ("refund_assets", assets_msg),
        ]))
}

pub fn withdraw_single_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: Asset,
    unmint_amount: Uint128,
    min_out_amount: Uint128,
) -> Result<Response, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let liquidity_addr: Addr = deps.api.addr_humanize(&pair_info.liquidity_token)?;

    let pools: Vec<Asset> = pair_info.query_pools(&deps.querier, deps.api, env.contract.address)?;
    let total_share: Uint128 = query_supply(&deps.querier, liquidity_addr)?;

    // TODO find better way for type conversion

    let old_balances: Vec<u128> = pools.iter().map(|a| a.amount.u128()).collect();
    let mut i = 0;
    let mut is_find = false;
    for (index, pool) in pools.iter().enumerate() {
        if asset.info.equal(&pool.info) {
            i = index;
            is_find = true;
            break;
        }
    }
    assert_eq!(is_find, true);

    let out_amount = Curve {
        amp: pair_info.amplification.u128() as u64,
        fee_numerator: pair_info.fee.u128() as u64,
    }
    .remove_liquidity_single_token(
        &old_balances,
        unmint_amount.u128() as u64,
        i as u8,
        total_share.u128() as u64,
    )
    .unwrap();

    assert_eq!(out_amount > min_out_amount.u128() as u64, true);

    let refund_asset = Asset {
        info: pools[i as usize].info.clone(),
        amount: Uint128::from(out_amount as u128),
    };

    // update pool info
    let amount = Uint128::from(unmint_amount);
    Ok(Response::new()
        .add_messages(vec![
            refund_asset
                .clone()
                .into_msg(&deps.querier, info.sender.clone())?,
            // burn liquidity token
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps
                    .api
                    .addr_humanize(&pair_info.liquidity_token)?
                    .to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
                funds: vec![],
            }),
        ])
        .add_attributes(vec![
            ("action", "withdraw_single_liquidity"),
            ("sender", info.sender.as_str()),
            ("withdrawn_share", &amount.to_string()),
            ("refund_asset", &format!("{}", refund_asset)),
        ]))
}

// CONTRACT - a user must do token approval
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    offer_asset: Asset,
    ask_asset: Asset,
    min_out_amount: Uint128,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    offer_asset.assert_sent_native_token_balance(&info)?;

    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;

    let pools: Vec<Asset> = pair_info.query_pools(&deps.querier, deps.api, env.contract.address)?;

    // let offer_pool: Asset;
    // let ask_pool: Asset;

    // If the asset balance is already increased
    // To calculated properly we should subtract user deposit from the pool
    // if offer_asset.info.equal(&pools[0].info) {
    //     offer_pool = Asset {
    //         amount: pools[0].amount.checked_sub(offer_asset.amount)?,
    //         info: pools[0].info.clone(),
    //     };
    //     ask_pool = pools[1].clone();
    // } else if offer_asset.info.equal(&pools[1].info) {
    //     offer_pool = Asset {
    //         amount: pools[1].amount.checked_sub(offer_asset.amount)?,
    //         info: pools[1].info.clone(),
    //     };
    //     ask_pool = pools[0].clone();
    // } else {
    //     return Err(ContractError::AssetMismatch {});
    // }

    let offer_amount = offer_asset.amount;
    let mut i = 0;
    let mut j = 0;
    // let mut offer_pool: Asset;
    let mut ask_pool = pools[0].clone();
    let mut balances = Vec::with_capacity(pools.len());
    for (index, pool) in pools.iter().enumerate() {
        if offer_asset.info.equal(&pool.info) {
            i = index;
            let amount = pool.amount.checked_sub(offer_asset.amount)?;
            // offer_pool = Asset {
            //     amount: amount,
            //     info: pool.info.clone(),
            // };
            balances.push(amount.u128());
            continue;
        }
        if ask_asset.info.equal(&pool.info) {
            j = index;
            ask_pool = pool.clone();
        }
        balances.push(pool.amount.u128());
    }
    let out_amount = Curve {
        amp: pair_info.amplification.u128() as u64,
        fee_numerator: pair_info.fee.u128() as u64,
    }
    .exchange(i, j, offer_amount.u128() as u64, &balances)
    .unwrap();
    let return_amount = Uint128::from(out_amount);

    // let return_amount = compute_swap(offer_pool.amount, ask_pool.amount, offer_amount);

    assert_eq!(return_amount.u128() > min_out_amount.u128(), true);
    // check max spread limit if exist
    // compute tax
    let return_asset = Asset {
        info: ask_pool.info.clone(),
        amount: return_amount,
    };
    let receiver = to.unwrap_or_else(|| sender.clone());
    let mut messages: Vec<CosmosMsg> = vec![];
    if !return_amount.is_zero() {
        messages.push(return_asset.into_msg(&deps.querier, receiver.clone())?);
    }

    // 1. send collateral token from the contract to a user
    // 2. send inactive commission to collector
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "swap"),
        ("sender", sender.as_str()),
        ("receiver", receiver.as_str()),
        ("offer_asset", &offer_asset.info.to_string()),
        ("ask_asset", &ask_pool.info.to_string()),
        ("offer_amount", &offer_amount.to_string()),
        ("return_amount", &return_amount.to_string()),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Pair {} => Ok(to_binary(&query_pair_info(deps)?)?),
        QueryMsg::Pool {} => Ok(to_binary(&query_pool(deps)?)?),
        // QueryMsg::Simulation { offer_asset } => {
        //     Ok(to_binary(&query_simulation(deps, offer_asset)?)?)
        // }
        // QueryMsg::ReverseSimulation { ask_asset } => {
        //     Ok(to_binary(&query_reverse_simulation(deps, ask_asset)?)?)
        // }
    }
}

pub fn query_pair_info(deps: Deps) -> Result<PairInfo, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let pair_info = pair_info.to_normal(deps.api)?;

    Ok(pair_info)
}

pub fn query_pool(deps: Deps) -> Result<PoolResponse, ContractError> {
    let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;
    let contract_addr = deps.api.addr_humanize(&pair_info.contract_addr)?;
    let assets: Vec<Asset> = pair_info.query_pools(&deps.querier, deps.api, contract_addr)?;
    let total_share: Uint128 = query_supply(
        &deps.querier,
        deps.api.addr_humanize(&pair_info.liquidity_token)?,
    )?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

// pub fn query_simulation(
//     deps: Deps,
//     offer_asset: Asset,
// ) -> Result<SimulationResponse, ContractError> {
//     let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;

//     let contract_addr = deps.api.addr_humanize(&pair_info.contract_addr)?;
//     let pools: Vec<Asset> = pair_info.query_pools(&deps.querier, deps.api, contract_addr)?;

//     let offer_pool: Asset;
//     let ask_pool: Asset;
//     if offer_asset.info.equal(&pools[0].info) {
//         offer_pool = pools[0].clone();
//         ask_pool = pools[1].clone();
//     } else if offer_asset.info.equal(&pools[1].info) {
//         offer_pool = pools[1].clone();
//         ask_pool = pools[0].clone();
//     } else {
//         return Err(ContractError::AssetMismatch {});
//     }

//     let return_amount = compute_swap(offer_pool.amount, ask_pool.amount, offer_asset.amount);

//     Ok(SimulationResponse { return_amount })
// }

// pub fn query_reverse_simulation(
//     deps: Deps,
//     ask_asset: Asset,
// ) -> Result<ReverseSimulationResponse, ContractError> {
//     let pair_info: PairInfoRaw = PAIR_INFO.load(deps.storage)?;

//     let contract_addr = deps.api.addr_humanize(&pair_info.contract_addr)?;
//     let pools: [Asset; 2] = pair_info.query_pools(&deps.querier, deps.api, contract_addr)?;

//     let offer_pool: Asset;
//     let ask_pool: Asset;
//     if ask_asset.info.equal(&pools[0].info) {
//         ask_pool = pools[0].clone();
//         offer_pool = pools[1].clone();
//     } else if ask_asset.info.equal(&pools[1].info) {
//         ask_pool = pools[1].clone();
//         offer_pool = pools[0].clone();
//     } else {
//         return Err(ContractError::AssetMismatch {});
//     }

//     let offer_amount = compute_swap(offer_pool.amount, ask_pool.amount, ask_asset.amount);

//     Ok(ReverseSimulationResponse { offer_amount })
// }

pub fn amount_of(coins: &[Coin], denom: String) -> Uint128 {
    match coins.iter().find(|x| x.denom == denom) {
        Some(coin) => coin.amount,
        None => Uint128::zero(),
    }
}

// fn compute_swap(offer_pool: Uint128, ask_pool: Uint128, offer_amount: Uint128) -> Uint128 {
//     let out_amount = Curve {
//         amp: AMPLIFICATION,
//         fee_numerator: FEE_NUMERATOR,
//     }
//     .exchange(
//         0,
//         1,
//         offer_amount.u128() as u64,
//         &[offer_pool.u128(), ask_pool.u128()],
//     )
//     .unwrap();
//     Uint128::from(out_amount)
// }

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
