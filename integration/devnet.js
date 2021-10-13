



var { LCDClient, Coin, LocalTerra, MsgStoreCode, MnemonicKey, MsgInstantiateContract, StdFee, MsgExecuteContract } = require('@terra-money/terra.js');
var fs = require('fs').promises;

const terra = new LCDClient({
    URL: 'https://bombay-lcd.terra.dev',
    chainID: 'bombay-12',
});

const mk = new MnemonicKey({
    mnemonic:
        'soul creek lizard budget once vibrant any ceiling voyage outdoor topple employ salon helmet hungry rival menu verb street base pact piano simple march',
});
const sender = "terra1fw0hvq2n8j4uavylqk8yercm2ssvflxhdyefjd"
const wallet = terra.wallet(mk);


async function deploy_terra_token_contract() {
    const data = await fs.readFile("../target/wasm32-unknown-unknown/release/terraswap_token.wasm", { encoding: 'base64' })
    var store_code = new MsgStoreCode(sender, data)
    var tx = await wallet
        .createAndSignTx({
            msgs: [store_code],
        })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
    return result.logs[0].events[1].attributes[1].value
}

async function deploy_terraswap_stable_contract() {
    const data = await fs.readFile("../target/wasm32-unknown-unknown/release/terraswap_stable.wasm", { encoding: 'base64' })
    var store_code = new MsgStoreCode(sender, data,)
    var tx = await wallet
        .createAndSignTx({
            msgs: [store_code],
            fee: new StdFee(10000000, "1500000uluna")
        })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
    return result.logs[0].events[1].attributes[1].value
}

async function init_token(decimals, symbol, supply, name, token_code_id) {
    var init_token_msg = new MsgInstantiateContract(
        sender,
        parseInt(token_code_id),
        {
            "decimals": parseInt(decimals),
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
        fee: new StdFee(10000000, "1500000uluna")
    })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
    return result.logs[0].events[0].attributes[3].value
}

async function init_pool(token_addrs, amp, fee, token_code_id, contract_code_id) {
    token_addrs = JSON.parse(token_addrs)
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
        parseInt(contract_code_id),
        {
            "asset_infos": asset_infos,
            "init_hook": {
                "contract_addr": "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf",
                "msg": "eyJyZWdpc3RlciI6eyJhc3NldF9pbmZvcyI6W3sibmF0aXZlX3Rva2VuIjp7ImRlbm9tIjoidXNkciJ9fSx7Im5hdGl2ZV90b2tlbiI6eyJkZW5vbSI6InVsdW5hIn19XX19"
            },
            "amplification": amp,
            "fee": fee,
            "token_code_id": parseInt(token_code_id)
        },
        {},
        false,
        sender,
        sender,
    )

    var tx = await wallet.createAndSignTx({
        msgs: [init_pool_msg],
        fee: new StdFee(10000000, "1500000uluna")
    })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
    console.log({
        "contract": result.logs[0].events[0].attributes[0].value,
        "lp_token": result.logs[0].events[0].attributes[1].value,
    })
}

async function add_liquidity(token_addrs, token_amounts, pool_address) {
    token_addrs = JSON.parse(token_addrs)
    token_amounts = JSON.parse(token_amounts)
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
        fee: new StdFee(10000000, "1500000uluna")
    })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
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
        fee: new StdFee(10000000, "1500000uluna")
    })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
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
        fee: new StdFee(10000000, "1500000uluna")
    })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
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
        fee: new StdFee(1000000, "150000uluna")
    })
    var result = await terra.tx.broadcast(tx)
    console.log("tx hash ", result.txhash)
    return result.txhash
}


// const someFunction = (param) => console.log('Welcome, your param is', param)

// exporting is crucial
module.exports = {
    deploy_terra_token_contract, deploy_terraswap_stable_contract,
    init_token, init_pool, add_liquidity, withdraw_liquidity, withdraw_single_liquidity, swap
}