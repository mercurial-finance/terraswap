var { LCDClient, Coin, LocalTerra, MsgStoreCode, MnemonicKey, MsgInstantiateContract, MsgExecuteContract } = require('@terra-money/terra.js');
var fs = require('fs').promises;

const terra = new LCDClient({
    URL: 'http://localhost:1317',
    chainID: 'localterra'
});

const mk = new MnemonicKey({
    mnemonic:
        'satisfy adjust timber high purchase tuition stool faith fine install that you unaware feed domain license impose boss human eager hat rent enjoy dawn',
});
const sender = "terra1dcegyrekltswvyy0xy69ydgxn9x8x32zdtapd8"
const wallet = terra.wallet(mk);


async function deploy_terra_token_contract() {
    const data = await fs.readFile("../target/wasm32-unknown-unknown/release/terraswap_token.wasm", { encoding: 'base64' })
    var store_code = new MsgStoreCode(sender, data)
    var tx = await wallet
        .createAndSignTx({
            msgs: [store_code],
        })
    var result = await terra.tx.broadcast(tx)
    return result.logs[0].events[1].attributes[1].value
}

async function deploy_terraswap_stable_contract() {
    const data = await fs.readFile("../target/wasm32-unknown-unknown/release/terraswap_stable.wasm", { encoding: 'base64' })
    var store_code = new MsgStoreCode(sender, data)
    var tx = await wallet
        .createAndSignTx({
            msgs: [store_code],
        })
    var result = await terra.tx.broadcast(tx)
    return result.logs[0].events[1].attributes[1].value
}

async function init_token(decimals, symbol, supply, name, token_code_id) {
    var init_token_msg = new MsgInstantiateContract(
        sender,
        token_code_id,
        {
            "decimals": decimals,
            "initial_balances": [{ "address": sender, "amount": supply }],
            "name": name,
            "symbol": symbol,
        },
        {},
        false,
        sender,
        sender,
    )

    var tx = await wallet.createAndSignTx({
        msgs: [init_token_msg],
    })
    var result = await terra.tx.broadcast(tx)

    return result.logs[0].events[0].attributes[3].value
}

async function init_pool(token_addrs, amp, fee, token_code_id, contract_code_id) {
    var asset_infos = []
    token_addrs.forEach(function (item, index, array) {
        asset_infos.push(
            {
                "token": {
                    "contract_addr": item
                }
            },
        )
    })
    var init_pool_msg = new MsgInstantiateContract(
        sender,
        contract_code_id,
        {
            "asset_infos": asset_infos,
            "init_hook": {
                "contract_addr": "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf",
                "msg": "eyJyZWdpc3RlciI6eyJhc3NldF9pbmZvcyI6W3sibmF0aXZlX3Rva2VuIjp7ImRlbm9tIjoidXNkciJ9fSx7Im5hdGl2ZV90b2tlbiI6eyJkZW5vbSI6InVsdW5hIn19XX19"
            },
            "amplification": amp,
            "fee": fee,
            "token_code_id": token_code_id
        },
        {},
        false,
        sender,
        sender,
    )

    var tx = await wallet.createAndSignTx({
        msgs: [init_pool_msg],
    })
    var result = await terra.tx.broadcast(tx)

    return {
        "contract": result.logs[0].events[0].attributes[0].value,
        "lp_token": result.logs[0].events[0].attributes[1].value,
    }
}

async function add_liquidity(token_addrs, token_amounts, pool_address) {
    var add_liquidity_msg = []
    var assets = []
    token_addrs.forEach(function (item, index, array) {
        add_liquidity_msg.push(
            new MsgExecuteContract(
                sender,
                item,
                {
                    "increase_allowance": {
                        "amount": token_amounts[index],
                        "spender": pool_address
                    }
                },
                {},
            )
        )

        assets.push(
            {
                "info": {
                    "token": {
                        "contract_addr": item
                    }
                },
                "amount": token_amounts[index]
            }
        )
    })

    add_liquidity_msg.push(new MsgExecuteContract(
        sender,
        pool_address,
        {
            "provide_liquidity": {
                "assets": assets,
                "min_out_amount": "0"
            }
        },
        {},
    ))

    var tx = await wallet.createAndSignTx({
        msgs: add_liquidity_msg,
    })
    var result = await terra.tx.broadcast(tx)

    return result.txhash
}


async function withdraw_liquidity(amount, pool_lp_token, pool_address) {
    var withdraw_msg = Buffer(JSON.stringify({
        "withdraw_liquidity": {
        }
    })).toString("base64");

    var withdraw_liquidity_msg = new MsgExecuteContract(
        sender,
        pool_lp_token,
        {
            "send": {
                "amount": amount,
                "contract": pool_address,
                "msg": withdraw_msg
            }
        }
    )

    var tx = await wallet.createAndSignTx({
        msgs: [withdraw_liquidity_msg],
    })
    var result = await terra.tx.broadcast(tx)

    return result.txhash
}


async function withdraw_single_liquidity(asset, amount, pool_lp_token, pool_address) {

    var withdraw_single_msg = Buffer(JSON.stringify({
        "withdraw_single_liquidity": {
            "asset": {
                "info": {
                    "token": {
                        "contract_addr": asset
                    }
                },
                "amount": "0"
            },
            "min_out_amount": "0"
        }
    })).toString("base64");

    var withdraw_liquidity_msg = new MsgExecuteContract(
        sender,
        pool_lp_token,
        {
            "send": {
                "amount": amount,
                "contract": pool_address,
                "msg": withdraw_single_msg
            }
        }
    )

    var tx = await wallet.createAndSignTx({
        msgs: [withdraw_liquidity_msg],
    })
    var result = await terra.tx.broadcast(tx)

    return result.txhash
}


async function swap(offer_asset, ask_asset, amount, pool_address) {
    var swap_usdt_msg = Buffer(JSON.stringify({
        "swap": {
            "ask_asset": {
                "info": {
                    "token": {
                        "contract_addr": ask_asset
                    }
                },
                "amount": "0"
            },
            "min_out_amount": "0"
        }
    })).toString("base64");

    var swap_msg = new MsgExecuteContract(
        sender,
        offer_asset,
        {
            "send": {
                "amount": amount,
                "contract": pool_address,
                "msg": swap_usdt_msg,
            }
        }
    )

    var tx = await wallet.createAndSignTx({
        msgs: [swap_msg],
    })
    var result = await terra.tx.broadcast(tx)

    return result.txhash
}





(async () => {
    try {
        var token_code_id = await deploy_terra_token_contract();
        token_code_id = parseInt(token_code_id)
        console.log("token code id ", token_code_id);

        var terra_stable_code_id = await deploy_terraswap_stable_contract();
        terra_stable_code_id = parseInt(terra_stable_code_id)
        console.log("contract code id ", terra_stable_code_id);

        var usdtCoin = await init_token(6, "USDT", "1000000", "tether", token_code_id)
        console.log("usdt address ", usdtCoin);
        var usdcCoin = await init_token(6, "USDC", "1000000", "usdc coin", token_code_id)
        console.log("usdc address ", usdcCoin);
        var daiCoin = await init_token(6, "DAI", "1000000", "dai", token_code_id)
        console.log("dai address ", daiCoin);
        var pool = await init_pool(
            [usdtCoin, usdcCoin, daiCoin], "60", "4", token_code_id, terra_stable_code_id
        )
        console.log("pool contract ", pool.contract);
        console.log("pool lp token ", pool.lp_token);

        var txHash = await add_liquidity([usdtCoin, usdcCoin, daiCoin], ["100", "100", "50"], pool.contract)
        console.log("add_liquidity tx ", txHash);

        var txHash = await withdraw_liquidity("10", pool.lp_token, pool.contract)
        console.log("withdraw_liquidity tx ", txHash);

        var txHash = await withdraw_single_liquidity(usdtCoin, "10", pool.lp_token, pool.contract)
        console.log("withdraw_liquidity tx ", txHash);

        var txHash = await swap(usdtCoin, daiCoin, "10", pool.contract)
        console.log("swap tx ", txHash);

    } catch (e) {
        console.log(e)
        // Deal with the fact the chain failed
    }
})();