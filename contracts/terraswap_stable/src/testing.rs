use crate::contract::{execute, instantiate, query_pair_info, query_pool, reply};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;

use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, to_binary, BankMsg, Coin, ContractResult, CosmosMsg, Decimal, Reply, ReplyOn, Response,
    StdError, SubMsg, SubMsgExecutionResponse, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, PoolResponse, ReverseSimulationResponse,
    SimulationResponse,
};
use terraswap::token::InstantiateMsg as TokenInstantiateMsg;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
        ],
        amplification: Uint128::from(60u128),
        fee: Uint128::from(4u128),
        token_code_id: 10u64,
    };

    // we can just call .unwrap() to assert this was a success
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Instantiate {
                code_id: 10u64,
                msg: to_binary(&TokenInstantiateMsg {
                    name: "terraswap liquidity token".to_string(),
                    symbol: "uLP".to_string(),
                    decimals: 6,
                    initial_balances: vec![],
                    mint: Some(MinterResponse {
                        minter: MOCK_CONTRACT_ADDR.to_string(),
                        cap: None,
                    }),
                })
                .unwrap(),
                funds: vec![],
                label: "".to_string(),
                admin: None,
            }
            .into(),
            gas_limit: None,
            id: 1,
            reply_on: ReplyOn::Success,
        }]
    );

    // store liquidity token
    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                vec![
                    10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
                ]
                .into(),
            ),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // it worked, let's query the state
    let pair_info: PairInfo = query_pair_info(deps.as_ref()).unwrap();
    assert_eq!("liquidity0000", pair_info.liquidity_token.as_str());
    assert_eq!(
        pair_info.asset_infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: "asset0000".to_string()
            }
        ]
    );
}

#[test]
fn provide_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::from(100u128),
    }]);

    deps.querier.with_token_balances(&[
        (
            &"liquidity0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::zero())],
        ),
        (&"asset0000".to_string(), &[]),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
        amplification: Uint128::from(60u128),
        fee: Uint128::from(4u128),
        token_code_id: 10u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // store liquidity token
    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                vec![
                    10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
                ]
                .into(),
            ),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // successfully provide liquidity for the exist pool
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        min_out_amount: Uint128::from(0u128),
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100u128),
        }],
    );
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let transfer_from_msg = res.messages.get(0).expect("no message");
    let mint_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: "addr0000".to_string(),
                recipient: MOCK_CONTRACT_ADDR.to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))
    );
    assert_eq!(
        mint_msg,
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "liquidity0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(200u128),
            })
            .unwrap(),
            funds: vec![],
        }))
    );

    // provide more liquidity 1:2, which is not proportional to 1:1,
    // then it must accept 1:1 and treat left amount as donation
    deps.querier.with_balance(&[(
        &MOCK_CONTRACT_ADDR.to_string(),
        vec![Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(
                200u128 + 200u128, /* user deposit must be pre-applied */
            ),
        }],
    )]);

    deps.querier.with_token_balances(&[
        (
            &"liquidity0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100u128))],
        ),
        (
            &"asset0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(200u128))],
        ),
    ]);

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(200u128),
            },
        ],
        min_out_amount: Uint128::from(0u128),
        receiver: Some("staking0000".to_string()), // try changing receiver
    };

    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(200u128),
        }],
    );

    // only accept 100, then 50 share will be generated with 100 * (100 / 200)
    // let res: Response = execute(deps.as_mut(), env, info, msg).unwrap();
    // let transfer_from_msg = res.messages.get(0).expect("no message");
    // let mint_msg = res.messages.get(1).expect("no message");
    // assert_eq!(
    //     transfer_from_msg,
    //     &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
    //         contract_addr: "asset0000".to_string(),
    //         msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
    //             owner: "addr0000".to_string(),
    //             recipient: MOCK_CONTRACT_ADDR.to_string(),
    //             amount: Uint128::from(100u128),
    //         })
    //         .unwrap(),
    //         funds: vec![],
    //     }))
    // );
    // assert_eq!(
    //     mint_msg,
    //     &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
    //         contract_addr: "liquidity0000".to_string(),
    //         msg: to_binary(&Cw20ExecuteMsg::Mint {
    //             recipient: "staking0000".to_string(), // LP tokens sent to specified receiver
    //             amount: Uint128::from(50u128),
    //         })
    //         .unwrap(),
    //         funds: vec![],
    //     }))
    // );

    // check wrong argument
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(50u128),
            },
        ],
        min_out_amount: Uint128::from(0u128),
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100u128),
        }],
    );
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    match res {
        ContractError::Std(StdError::GenericErr { msg, .. }) => assert_eq!(
            msg,
            "Native token balance mismatch between the argument and the transferred".to_string()
        ),
        _ => panic!("Must return generic error"),
    }

    // initialize token balance to 1:1
    deps.querier.with_balance(&[(
        &MOCK_CONTRACT_ADDR.to_string(),
        vec![Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(
                100u128 + 100u128, /* user deposit must be pre-applied */
            ),
        }],
    )]);

    deps.querier.with_token_balances(&[
        (
            &"liquidity0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100u128))],
        ),
        (
            &"asset0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100u128))],
        ),
    ]);

    // failed because the price is under slippage_tolerance
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(98u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        min_out_amount: Uint128::from(200u128),
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100u128),
        }],
    );
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    match res {
        ContractError::MaxSlippageAssertion {} => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    // initialize token balance to 1:1
    deps.querier.with_balance(&[(
        &MOCK_CONTRACT_ADDR.to_string(),
        vec![Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100u128 + 98u128 /* user deposit must be pre-applied */),
        }],
    )]);

    // failed because the price is under slippage_tolerance
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(98u128),
            },
        ],
        min_out_amount: Uint128::from(200u128),
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(98u128),
        }],
    );
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    match res {
        ContractError::MaxSlippageAssertion {} => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    // initialize token balance to 1:1
    deps.querier.with_balance(&[(
        &MOCK_CONTRACT_ADDR.to_string(),
        vec![Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(
                100u128 + 100u128, /* user deposit must be pre-applied */
            ),
        }],
    )]);

    // successfully provides
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(99u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        min_out_amount: Uint128::from(0u128),
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100u128),
        }],
    );
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();

    // initialize token balance to 1:1
    deps.querier.with_balance(&[(
        &MOCK_CONTRACT_ADDR.to_string(),
        vec![Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100u128 + 99u128 /* user deposit must be pre-applied */),
        }],
    )]);

    // successfully provides
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(99u128),
            },
        ],
        min_out_amount: Uint128::from(0u128),
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(99u128),
        }],
    );
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();
}

// #[test]
// fn withdraw_single_liquidity() {
//     let mut deps = mock_dependencies(&[Coin {
//         denom: "uusd".to_string(),
//         amount: Uint128::from(100u128),
//     }]);

//     deps.querier.with_tax(
//         Decimal::zero(),
//         &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
//     );
//     deps.querier.with_token_balances(&[
//         (
//             &"liquidity0000".to_string(),
//             &[(&"addr0000".to_string(), &Uint128::from(100u128))],
//         ),
//         (
//             &"asset0000".to_string(),
//             &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100u128))],
//         ),
//     ]);

//     let msg = InstantiateMsg {
//         asset_infos: vec![
//             AssetInfo::NativeToken {
//                 denom: "uusd".to_string(),
//             },
//             AssetInfo::Token {
//                 contract_addr: "asset0000".to_string(),
//             },
//         ],
//         amplification: Uint128::from(60u128),
//         fee: Uint128::from(4u128),
//         token_code_id: 10u64,
//     };

//     let env = mock_env();
//     let info = mock_info("addr0000", &[]);
//     // we can just call .unwrap() to assert this was a success
//     let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

//     // store liquidity token
//     let reply_msg = Reply {
//         id: 1,
//         result: ContractResult::Ok(SubMsgExecutionResponse {
//             events: vec![],
//             data: Some(
//                 vec![
//                     10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
//                 ]
//                 .into(),
//             ),
//         }),
//     };

//     let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

//     let msg = ExecuteMsg::WithdrawSingleLiquidity {
//         asset: Asset {
//             info: AssetInfo::NativeToken {
//                 denom: "uusd".to_string(),
//             },
//             amount: Uint128::from(0u128),
//         },
//         unmint_amount: Uint128::from(100u128),
//         min_out_amount: Uint128::from(0u128),
//     };

//     let env = mock_env();
//     let info = mock_info("liquidity0000", &[]);
//     let res = execute(deps.as_mut(), env, info, msg).unwrap();
//     let log_withdrawn_share = res.attributes.get(2).expect("no log");
//     let log_refund_assets = res.attributes.get(3).expect("no log");
//     let msg_refund_0 = res.messages.get(0).expect("no message");
//     let msg_burn_liquidity = res.messages.get(1).expect("no message");
//     assert_eq!(
//         msg_refund_0,
//         &SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
//             to_address: "liquidity0000".to_string(),
//             amount: vec![Coin {
//                 denom: "uusd".to_string(),
//                 amount: Uint128::from(99u128),
//             }],
//         }))
//     );
//     assert_eq!(
//         msg_burn_liquidity,
//         &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: "liquidity0000".to_string(),
//             msg: to_binary(&Cw20ExecuteMsg::Burn {
//                 amount: Uint128::from(100u128),
//             })
//             .unwrap(),
//             funds: vec![],
//         }))
//     );

//     assert_eq!(
//         log_withdrawn_share,
//         &attr("withdrawn_share", 100u128.to_string())
//     );
//     assert_eq!(log_refund_assets, &attr("refund_asset", "99uusd"));
// }

#[test]
fn withdraw_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::from(100u128),
    }]);

    deps.querier.with_tax(
        Decimal::zero(),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );
    deps.querier.with_token_balances(&[
        (
            &"liquidity0000".to_string(),
            &[(&"addr0000".to_string(), &Uint128::from(100u128))],
        ),
        (
            &"asset0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100u128))],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
        ],
        amplification: Uint128::from(60u128),
        fee: Uint128::from(4u128),
        token_code_id: 10u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // store liquidity token
    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                vec![
                    10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
                ]
                .into(),
            ),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // withdraw single liquidity
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
        amount: Uint128::from(100u128),
    });

    let env = mock_env();
    let info = mock_info("liquidity0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let log_withdrawn_share = res.attributes.get(2).expect("no log");
    // let log_refund_assets = res.attributes.get(3).expect("no log");
    let msg_refund_0 = res.messages.get(0).expect("no message");
    let msg_refund_1 = res.messages.get(1).expect("no message");
    let msg_burn_liquidity = res.messages.get(2).expect("no message");
    assert_eq!(
        msg_refund_0,
        &SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
            to_address: "addr0000".to_string(),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(100u128),
            }],
        }))
    );
    assert_eq!(
        msg_refund_1,
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))
    );
    assert_eq!(
        msg_burn_liquidity,
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "liquidity0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))
    );

    assert_eq!(
        log_withdrawn_share,
        &attr("withdrawn_share", 100u128.to_string())
    );
    // assert_eq!(
    //     log_refund_assets,
    //     &attr("refund_assets", "100uusd, 100asset0000")
    // );
}

#[test]
fn try_native_to_token() {
    let total_share = Uint128::from(30000000000u128);
    let asset_pool_amount = Uint128::from(20000000000u128);
    let collateral_pool_amount = Uint128::from(30000000000u128);
    let exchange_rate: Decimal = Decimal::from_ratio(asset_pool_amount, collateral_pool_amount);
    let offer_amount = Uint128::from(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount + offer_amount, /* user deposit must be pre-applied */
    }]);

    deps.querier.with_tax(
        Decimal::zero(),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    deps.querier.with_token_balances(&[
        (
            &"liquidity0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &total_share)],
        ),
        (
            &"asset0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &asset_pool_amount)],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
        ],
        amplification: Uint128::from(60u128),
        fee: Uint128::from(4u128),
        token_code_id: 10u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // store liquidity token
    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                vec![
                    10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
                ]
                .into(),
            ),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // normal swap
    let msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: offer_amount,
        },
        ask_asset: Asset {
            info: AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
            amount: Uint128::from(0u128),
        },
        min_out_amount: Uint128::zero(),
        to: None,
    };
    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: offer_amount,
        }],
    );
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let msg_transfer = res.messages.get(0).expect("no message");

    let expected_return_amount = Uint128::from(1486872698u128);

    // check simulation res
    // deps.querier.with_balance(&[(
    //     &MOCK_CONTRACT_ADDR.to_string(),
    //     vec![Coin {
    //         denom: "uusd".to_string(),
    //         amount: collateral_pool_amount, /* user deposit must be pre-applied */
    //     }],
    // )]);

    // let simulation_res: SimulationResponse = query_simulation(
    //     deps.as_ref(),
    //     Asset {
    //         info: AssetInfo::NativeToken {
    //             denom: "uusd".to_string(),
    //         },
    //         amount: offer_amount,
    //     },
    // )
    // .unwrap();
    // assert_eq!(expected_return_amount, simulation_res.return_amount);

    // check reverse simulation res
    // let reverse_simulation_res: ReverseSimulationResponse = query_reverse_simulation(
    //     deps.as_ref(),
    //     Asset {
    //         info: AssetInfo::Token {
    //             contract_addr: "asset0000".to_string(),
    //         },
    //         amount: expected_return_amount,
    //     },
    // )
    // .unwrap();

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "uusd"),
            attr("ask_asset", "asset0000"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_return_amount.to_string()),
        ]
    );

    assert_eq!(
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "asset0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: expected_return_amount,
            })
            .unwrap(),
            funds: vec![],
        })),
        msg_transfer,
    );
}

#[test]
fn try_token_to_native() {
    let total_share = Uint128::from(20000000000u128);
    let asset_pool_amount = Uint128::from(30000000000u128);
    let collateral_pool_amount = Uint128::from(20000000000u128);
    let exchange_rate = Decimal::from_ratio(collateral_pool_amount, asset_pool_amount);
    let offer_amount = Uint128::from(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount,
    }]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );
    deps.querier.with_token_balances(&[
        (
            &"liquidity0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &total_share)],
        ),
        (
            &"asset0000".to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &(asset_pool_amount + offer_amount),
            )],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
        ],
        amplification: Uint128::from(60u128),
        fee: Uint128::from(4u128),
        token_code_id: 10u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // store liquidity token
    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                vec![
                    10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
                ]
                .into(),
            ),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // unauthorized access; can not execute swap directly for token swap
    let msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
            amount: offer_amount,
        },
        ask_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(0u128),
        },
        min_out_amount: Uint128::zero(),
        to: None,
    };
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    match res {
        ContractError::Unauthorized {} => (),
        _ => panic!("DO NOT ENTER HERE"),
    }

    // normal sell
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            ask_asset: Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(0u128),
            },
            min_out_amount: Uint128::zero(),
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env();
    let info = mock_info("asset0000", &[]);

    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let msg_transfer = res.messages.get(0).expect("no message");

    // current price is 1.5, so expected return without spread is 1000
    // 952.380952 = 20000 - 20000 * 30000 / (30000 + 1500)
    let expected_return_amount = Uint128::from(1486872698u128);
    // check simulation res
    // return asset token balance as normal
    // deps.querier.with_token_balances(&[
    //     (
    //         &"liquidity0000".to_string(),
    //         &[(&MOCK_CONTRACT_ADDR.to_string(), &total_share)],
    //     ),
    //     (
    //         &"asset0000".to_string(),
    //         &[(&MOCK_CONTRACT_ADDR.to_string(), &(asset_pool_amount))],
    //     ),
    // ]);

    // let simulation_res: SimulationResponse = query_simulation(
    //     deps.as_ref(),
    //     Asset {
    //         amount: offer_amount,
    //         info: AssetInfo::Token {
    //             contract_addr: "asset0000".to_string(),
    //         },
    //     },
    // )
    // .unwrap();
    // assert_eq!(expected_return_amount, simulation_res.return_amount);

    // // check reverse simulation res
    // let reverse_simulation_res: ReverseSimulationResponse = query_reverse_simulation(
    //     deps.as_ref(),
    //     Asset {
    //         amount: expected_return_amount,
    //         info: AssetInfo::NativeToken {
    //             denom: "uusd".to_string(),
    //         },
    //     },
    // )
    // .unwrap();

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "asset0000"),
            attr("ask_asset", "uusd"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_return_amount.to_string()),
        ]
    );

    // failed due to non asset token contract try to execute sell
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            ask_asset: Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(0u128),
            },
            min_out_amount: Uint128::zero(),
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env();
    let info = mock_info("liquidity0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    match res {
        ContractError::Unauthorized {} => (),
        _ => panic!("DO NOT ENTER HERE"),
    }
}

// #[test]
// fn test_max_spread() {
//     assert_max_spread(
//         Some(Decimal::from_ratio(1200u128, 1u128)),
//         Some(Decimal::percent(1)),
//         Uint128::from(1200000000u128),
//         Uint128::from(989999u128),
//         Uint128::zero(),
//     )
//     .unwrap_err();

//     assert_max_spread(
//         Some(Decimal::from_ratio(1200u128, 1u128)),
//         Some(Decimal::percent(1)),
//         Uint128::from(1200000000u128),
//         Uint128::from(990000u128),
//         Uint128::zero(),
//     )
//     .unwrap();

//     assert_max_spread(
//         None,
//         Some(Decimal::percent(1)),
//         Uint128::zero(),
//         Uint128::from(989999u128),
//         Uint128::from(10001u128),
//     )
//     .unwrap_err();

//     assert_max_spread(
//         None,
//         Some(Decimal::percent(1)),
//         Uint128::zero(),
//         Uint128::from(990000u128),
//         Uint128::from(10000u128),
//     )
//     .unwrap();
// }

// #[test]
// fn test_deduct() {
//     let mut deps = mock_dependencies(&[]);

//     let tax_rate = Decimal::percent(2);
//     let tax_cap = Uint128::from(1_000_000u128);
//     deps.querier.with_tax(
//         Decimal::percent(2),
//         &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
//     );

//     let amount = Uint128::from(1_000_000_000u128);
//     let expected_after_amount = std::cmp::max(
//         amount.checked_sub(amount * tax_rate).unwrap(),
//         amount.checked_sub(tax_cap).unwrap(),
//     );

//     let after_amount = (Asset {
//         info: AssetInfo::NativeToken {
//             denom: "uusd".to_string(),
//         },
//         amount,
//     })
//     .deduct_tax(&deps.as_ref().querier)
//     .unwrap();

//     assert_eq!(expected_after_amount, after_amount.amount);
// }

#[test]
fn test_query_pool() {
    let total_share_amount = Uint128::from(111u128);
    let asset_0_amount = Uint128::from(222u128);
    let asset_1_amount = Uint128::from(333u128);
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    deps.querier.with_token_balances(&[
        (
            &"asset0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &asset_1_amount)],
        ),
        (
            &"liquidity0000".to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &total_share_amount)],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: "asset0000".to_string(),
            },
        ],
        amplification: Uint128::from(60u128),
        fee: Uint128::from(4u128),
        token_code_id: 10u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // store liquidity token
    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                vec![
                    10, 13, 108, 105, 113, 117, 105, 100, 105, 116, 121, 48, 48, 48, 48,
                ]
                .into(),
            ),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    let res: PoolResponse = query_pool(deps.as_ref()).unwrap();

    assert_eq!(
        res.assets,
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: asset_0_amount
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset0000".to_string(),
                },
                amount: asset_1_amount
            }
        ]
    );
    assert_eq!(res.total_share, total_share_amount);
}
