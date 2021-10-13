**Localnet**

```javascript
node local_test.js
```


**Devnet**

Deploy contract

```javascript
npx run-func devnet.js deploy_terraswap_stable_contract
```

Init token

```javascript
npx run-func devnet.js init_token 6 "USDT" "1000000" "tether" 148

npx run-func devnet.js init_token 6 "USDC" "1000000" "usdc coin" 148

npx run-func devnet.js init_token 6 "DAI" "1000000" "market dao" 148
```

Init pool

```javascript
npx run-func devnet.js init_pool "[\"terra1ea9tmdpdkznvgk9edea020vlwx03vln9qy82w7\",\"terra1pes7h59ck9sluhu0a4q4kga3fd63afcufkx7ht\", \"terra1f4xsuzmc8h4raxlvrxcttyvpzqsaq0esrc5utj\"]" "60" "4" 148 13134
```

Add liquidity

```javascript
npx run-func devnet.js add_liquidity "[\"terra1ea9tmdpdkznvgk9edea020vlwx03vln9qy82w7\",\"terra1pes7h59ck9sluhu0a4q4kga3fd63afcufkx7ht\", \"terra1f4xsuzmc8h4raxlvrxcttyvpzqsaq0esrc5utj\"]" "[\"10000\", \"10000\", \"10000\"]" "terra14dg0l7yqlvpazpczmfd7shnptklk20qnkt5700"
```

Remove Liquidity

```javascript
npx run-func devnet.js withdraw_liquidity "100" "terra1uxv6jrrst948m2vt6z97ttwp3czql328quw6ke" "terra14dg0l7yqlvpazpczmfd7shnptklk20qnkt5700"
```

Remove Single Liquidity

```javascript
npx run-func devnet.js withdraw_single_liquidity "terra1ea9tmdpdkznvgk9edea020vlwx03vln9qy82w7" "100" "terra1uxv6jrrst948m2vt6z97ttwp3czql328quw6ke" "terra14dg0l7yqlvpazpczmfd7shnptklk20qnkt5700"
```

Swap 

```javascript
npx run-func devnet.js swap "terra1ea9tmdpdkznvgk9edea020vlwx03vln9qy82w7" "terra1f4xsuzmc8h4raxlvrxcttyvpzqsaq0esrc5utj" "100" "terra14dg0l7yqlvpazpczmfd7shnptklk20qnkt5700"
```