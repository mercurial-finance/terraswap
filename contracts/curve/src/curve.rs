use spl_math::uint::U256;
use std::convert::TryInto;

const ITERATIONS: u8 = 32;
pub const FEE_DENOMINATOR: u64 = 10_000;

pub struct Curve {
    pub amp: u64,
    pub fee_numerator: u64,
}

impl Curve {
    pub fn reverse_exchange(
        &self,
        i: usize,
        j: usize,
        out_amount: u128,
        balances: &[u128],
    ) -> Option<u128> {
        let out_amount_after_fee = u128::from(out_amount)
            .checked_mul(FEE_DENOMINATOR as u128)?
            .checked_div(FEE_DENOMINATOR as u128 - self.fee_numerator as u128)?;

        let x = balances[i as usize].checked_sub(out_amount_after_fee)?;

        let y: u128 = self.get_y(i, j, x, &balances)?;

        let dy = y
            .checked_sub(balances[j as usize].try_into().ok()?)?
            .checked_add(1u128);

        return dy;
    }

    pub fn get_d(&self, amounts: &[u128], d_suggest: Option<u128>) -> Option<u128> {
        let n_coins = amounts.len();
        let sum_x: u128 = amounts.iter().sum();
        if sum_x == 0 {
            return Some(0u128);
        }

        let amounts_times_coin: Vec<u128> = amounts
            .iter()
            .map(|amount| amount.checked_mul(n_coins as u128).unwrap())
            .collect();

        let ann: u64 = self.amp.checked_mul(n_coins as u64)?;
        let ann_mul_sum_x = U256::from(ann).checked_mul(sum_x.into())?;
        let ann_sub_one = U256::from(ann.checked_sub(1u64)?);

        let mut d_prev: u128;
        let mut d = d_suggest.unwrap_or(sum_x.into());
        for i in 0..ITERATIONS {
            let d_u256 = U256::from(d);
            let mut d_prod: U256 = d_u256;
            for amount_time_coin in amounts_times_coin.iter() {
                d_prod = d_prod
                    .checked_mul(d_u256)?
                    .checked_div(U256::from(*amount_time_coin))?;
            }
            d_prev = d;
            // d = (ann * sum_x + d_prod * n_coins) * d / ((d * (ann - 1)) + (d_prod * (n_coins + 1)))
            let d_prod_mul_n_coins = d_prod.checked_mul((n_coins as u64).into())?;
            let numerator = (ann_mul_sum_x.checked_add(d_prod_mul_n_coins)?).checked_mul(d_u256)?;
            let denominator = (d_u256.checked_mul(ann_sub_one)?)
                .checked_add(d_prod.checked_add(d_prod_mul_n_coins)?)?;
            d = numerator.checked_div(denominator)?.try_into().ok()?;

            // Equality with the precision of 1
            if d > d_prev {
                if d.checked_sub(d_prev)? <= 1u128 {
                    break;
                }
            } else if d_prev.checked_sub(d)? <= 1u128 {
                break;
            }
        }
        Some(d)
    }

    /// Get x[i] if one reduces D from being calculated for xp to D
    /// Solve iteratively:
    /// x_1**2 + x_1 * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n + 1) / (n ** (2 * n) * prod' * A)
    /// x_1**2 + b*x_1 = c
    /// x_1 = (x_1**2 + c) / (2*x_1 + b)
    pub fn get_y_d(&self, i: u8, amounts: &[u128], d: u128) -> Option<u128> {
        let n_coins = amounts.len();
        let ann: u64 = self.amp.checked_mul(n_coins as u64)?;

        let mut c: U256 = U256::from(d);
        let mut s = 0u128;

        for k in 0..n_coins {
            let x_temp = if k != i.into() {
                amounts[k]
            } else {
                continue;
            };

            s = s.checked_add(x_temp.into())?;
            c = c
                .checked_mul(d.into())?
                .checked_div(x_temp.checked_mul(n_coins as u128)?.into())?;
        }
        c = c
            .checked_mul(d.into())?
            .checked_div(ann.checked_mul(n_coins as u64)?.into())?;

        // TODO: Refactor to share with get_y, as this is identical
        let b: u128 = s.checked_add(d.checked_div(ann.into())?)?;
        let mut y = d;
        for _ in 0..ITERATIONS {
            let y_prev = y;
            let y_numerator = y.checked_mul(y)?.checked_add(c.try_into().ok()?)?;
            let y_denominator = y.checked_mul(2u128)?.checked_add(b)?.checked_sub(d)?;
            y = y_numerator.checked_div(y_denominator)?;
            if y > y_prev {
                if y.checked_sub(y_prev)? <= 1u128 {
                    break;
                }
            } else {
                if y_prev.checked_sub(y)? <= 1u128 {
                    break;
                }
            }
        }

        Some(y)
    }

    /// Get swap amount `y` in proportion to `x`
    /// Solve for y:
    /// y**2 + y * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n + 1) / (n ** (2 * n) * prod' * A)
    /// y**2 + b*y = c
    pub fn get_y(&self, i: usize, j: usize, x: u128, balances: &[u128]) -> Option<u128> {
        let n_coins = balances.len();
        let d = self.get_d(balances, None)?;
        let ann: u64 = self.amp.checked_mul(n_coins as u64)?;

        let mut c: U256 = U256::from(d);
        let mut s: u128 = 0u128;

        for (k, balance) in balances.iter().enumerate() {
            let x_temp = if k == i.into() {
                x
            } else if k != j.into() {
                balance.clone()
            } else {
                continue;
            };

            s = s.checked_add(x_temp.into())?;
            c = c
                .checked_mul(d.into())?
                .checked_div(x_temp.checked_mul(n_coins as u128)?.into())?;
        }
        c = c
            .checked_mul(d.into())?
            .checked_div(ann.checked_mul(n_coins as u64)?.into())?;

        let b: u128 = s.checked_add(d.checked_div(ann.into())?)?;
        let mut y = d;
        for _ in 0..ITERATIONS {
            let y_prev = y;
            let y_numerator = y.checked_mul(y)?.checked_add(c.try_into().ok()?)?;
            let y_denominator = y.checked_mul(2u128)?.checked_add(b)?.checked_sub(d)?;
            y = y_numerator.checked_div(y_denominator)?;
            if y > y_prev {
                if y.checked_sub(y_prev)? <= 1u128 {
                    break;
                }
            } else {
                if y_prev.checked_sub(y)? <= 1u128 {
                    break;
                }
            }
        }

        Some(y)
    }

    pub fn get_virtual_price(
        &self,
        balances: &[u128],
        lp_token_total: u64,
        precision_factor: u8,
    ) -> Option<u64> {
        let d = self.get_d(balances, None)?;
        let virtual_price = d
            .checked_mul(10u128.checked_pow(precision_factor as u32)?)?
            .checked_div(lp_token_total.into())?;

        virtual_price.try_into().ok()
    }

    pub fn deposit(
        &self,
        old_balances: &[u128],
        new_balances: &[u128],
        lp_token_total: u64,
    ) -> Option<u64> {
        let n_coins = old_balances.len();
        let d_0 = self.get_d(old_balances, None)?;
        let d_1 = self.get_d(new_balances, None)?;

        if d_1 <= d_0 {
            return None;
        }

        if lp_token_total <= 0 {
            return Some(d_1.try_into().ok()?);
        }
        let fee = self
            .fee_numerator
            .checked_mul(n_coins as u64)?
            .checked_div(4u64)?
            .checked_div(n_coins as u64 - 1)?;
        let mut new_balances_after_deducted_fee = Vec::with_capacity(n_coins);
        for i in 0..n_coins {
            let old_balance = old_balances[i];
            let new_balance = new_balances[i];
            let ideal_balance: u128 = d_1
                .checked_mul(old_balance.into())?
                .checked_div(d_0)?
                .try_into()
                .ok()?;
            let difference = if ideal_balance < new_balance {
                new_balance.checked_sub(ideal_balance)?
            } else {
                ideal_balance.checked_sub(new_balance)?
            };
            let fee_for_token = u128::from(fee)
                .checked_mul(difference.into())?
                .checked_div(FEE_DENOMINATOR.into())?
                .try_into()
                .ok()?;
            new_balances_after_deducted_fee.push(new_balance.checked_sub(fee_for_token)?);
        }

        let new_sum_x: u128 = new_balances.iter().sum();
        let fee_sum_x: u128 = new_balances_after_deducted_fee.iter().sum();

        let d_suggest = u128::from(fee_sum_x)
            .checked_mul(d_1)?
            .checked_div(new_sum_x.into())?;
        let d_2 = self.get_d(&new_balances_after_deducted_fee, Some(d_suggest))?;
        let d_diff = d_2.checked_sub(d_0)?;
        let mint_amount = u128::from(lp_token_total)
            .checked_mul(d_diff)?
            .checked_div(d_0)?;

        Some(mint_amount.try_into().ok()?)
    }

    pub fn remove_balanced_liquidity(
        old_balances: &[u128],
        unmint_amount: u64,
        lp_total_supply: u64,
    ) -> Option<Vec<u128>> {
        let mut amounts = Vec::with_capacity(old_balances.len());

        for old_balance in old_balances.iter() {
            let amount: u128 = u128::from(*old_balance)
                .checked_mul(unmint_amount.into())?
                .checked_div(lp_total_supply.into())?
                .try_into()
                .ok()?;
            amounts.push(amount);
        }

        Some(amounts)
    }

    pub fn remove_liquidity_single_token(
        &self,
        old_balances: &[u128],
        unmint_amount: u64,
        i: u8,
        lp_total_supply: u64,
    ) -> Option<u64> {
        let n_coins = old_balances.len();

        let d0 = self.get_d(&old_balances, None)?;
        let d1 = d0.checked_sub(
            u128::from(unmint_amount)
                .checked_mul(d0)?
                .checked_div(lp_total_supply.into())?,
        )?;
        let new_y = self.get_y_d(i, &old_balances, d1)?;
        let mut xp_reduced = Vec::with_capacity(n_coins);

        let fee = self
            .fee_numerator
            .checked_mul(n_coins as u64)?
            .checked_div(4u64)?
            .checked_div(n_coins as u64 - 1)?;

        for j in 0..n_coins {
            let old_balance_u128: u128 = old_balances[j].into();
            let dx_expected = if j == i.into() {
                old_balance_u128
                    .checked_mul(d1)?
                    .checked_div(d0)?
                    .checked_sub(new_y)?
            } else {
                old_balance_u128.checked_sub(old_balance_u128.checked_mul(d1)?.checked_div(d0)?)?
            };
            xp_reduced.push(
                old_balances[j].checked_sub(
                    u128::from(fee)
                        .checked_mul(dx_expected.into())?
                        .checked_div(FEE_DENOMINATOR.into())?
                        .try_into()
                        .ok()?,
                )?,
            );
        }

        let mut dy = xp_reduced[i as usize].checked_sub(self.get_y_d(i, &xp_reduced, d1)?)?;
        // Curve minuses 1 from the final amount. This helps to solve 0 to 1 withdrawal.
        // Source: https://github.com/curvefi/curve-contract/blob/master/contracts/pools/3pool/StableSwap3Pool.vy#L657
        dy = dy.checked_sub(1u128)?;

        Some(dy.try_into().ok()?)
    }

    pub fn exchange(&self, i: usize, j: usize, in_amount: u64, balances: &[u128]) -> Option<u128> {
        let x = balances[i as usize].checked_add(in_amount.try_into().ok()?)?;
        let y = self.get_y(i, j, x, &balances)?;
        // Curve minuses 1 from the final amount. This helps to solve 0 to 1 swapping.
        // Source: https://github.com/curvefi/curve-contract/blob/master/contracts/pools/3pool/StableSwap3Pool.vy#L465
        let mut dy: u128 = balances[j as usize]
            .checked_sub(y.try_into().ok()?)?
            .checked_sub(1u128)?;
        let dy_fee: u128 = u128::from(dy)
            .checked_mul(self.fee_numerator.into())?
            .checked_div(FEE_DENOMINATOR.into())?;
        dy = dy.checked_sub(dy_fee)?;
        Some(dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::rngs::StdRng;
    use rand::Rng;
    use simulation::Simulation;
    const SEED: u64 = 123456; // Allows for reproducible failure should there be any

    const N_COINS: usize = 3;
    const AMPLIFICATION_COEFFICIENT: u64 = 2000u64;
    const TRADING_FEE: u64 = 4u64;

    fn check_d(simulation: &Simulation, curve: &Curve, amounts: &[u128]) -> u128 {
        let d = curve.get_d(amounts, None).unwrap();
        assert_eq!(d, simulation.get_d().into());
        d
    }

    fn check_y(simulation: &Simulation, curve: &Curve, i: u8, j: u8, x: u128, balances: &[u128]) {
        let y = curve.get_y(i as usize, j as usize, x, balances).unwrap();

        assert_eq!(
            y,
            simulation
                .get_y(i.into(), j.into(), x.try_into().unwrap())
                .into()
        )
    }

    fn check_y_d(simulation: &Simulation, curve: &Curve, i: u8, d: u128, balances: &[u128]) {
        let y = curve.get_y_d(i, &balances, d).unwrap();
        assert_eq!(
            y,
            simulation.get_y_d(i.into(), d.try_into().unwrap()).into()
        )
    }

    #[test]
    fn test_get_d() {
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let precision_multipliers = vec![1u64, 1u64, 1u64];
        let simulation_no_balance = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            vec![0, 0, 0, 0],
            N_COINS as u8,
            None,
            &precision_multipliers,
        );
        check_d(
            &simulation_no_balance,
            &curve,
            &[0u128, 0u128, 0u128].to_vec(),
        );

        let amount_a: u128 = 1511922780182839;
        let amount_b: u128 = 1932395124083698;
        let amount_c: u128 = 1724082092237124;
        let amounts = vec![amount_a, amount_b, amount_c];
        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let simulation = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            amounts.clone(),
            N_COINS as u8,
            None,
            &precision_multipliers,
        );
        check_d(&simulation, &curve, &amounts);
    }

    #[test]
    fn test_get_mint_amount_for_adding_liquidity_with_random_input() {
        let mut rng: StdRng = rand::SeedableRng::seed_from_u64(SEED);
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        for _ in 0..100 {
            let old_amount_a: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let old_amount_b: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let old_amount_c: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));

            let precision_multipliers = vec![1, 1, 1];
            let lp_total_supply: u64 = check_adding_liquidity(
                &curve,
                &vec![0, 0, 0],
                &vec![old_amount_a, old_amount_b, old_amount_c],
                0,
                &precision_multipliers,
            );

            let new_amount_a: u128 = old_amount_a + rng.gen_range(0..u128::MAX / 10u128.pow(20));
            let new_amount_b: u128 = old_amount_b + rng.gen_range(0..u128::MAX / 10u128.pow(20));
            let new_amount_c: u128 = old_amount_c + rng.gen_range(0..u128::MAX / 10u128.pow(20));

            check_adding_liquidity(
                &curve,
                &vec![old_amount_a, old_amount_b, old_amount_c],
                &vec![new_amount_a, new_amount_b, new_amount_c],
                lp_total_supply,
                &precision_multipliers,
            );
        }
    }

    #[test]
    fn test_check_get_y() {
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let amount_a: u128 = 1511922780182839;
        let amount_b: u128 = 1932395124083698;
        let amount_c: u128 = 1224082092237124;
        let amount_x: u128 = 1128721179791137;
        let amounts = vec![amount_a, amount_b, amount_c];
        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let precision_multipliers = vec![1, 1, 1];
        let simulation = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            amounts.clone(),
            N_COINS as u8,
            None,
            &precision_multipliers,
        );
        check_y(&simulation, &curve, 0, 1, amount_x, &amounts);
    }

    #[test]
    fn test_get_y_d() {
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let amount_a: u128 = 1511922780182839;
        let amount_b: u128 = 1932395124083698;
        let amount_c: u128 = 1224082092237124;
        let amounts = vec![amount_a, amount_b, amount_c];
        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let precision_multipliers = vec![1, 1, 1];
        let simulation = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            amounts.clone(),
            N_COINS as u8,
            None,
            &precision_multipliers,
        );
        let d = check_d(&simulation, &curve, &amounts);
        check_y_d(&simulation, &curve, 0, d, &amounts);
    }

    #[test]
    fn test_curve_math_with_random_inputs() {
        let mut rng: StdRng = rand::SeedableRng::seed_from_u64(SEED);
        let precision_multipliers = vec![1u64, 1u64, 1u64];
        for _ in 0..1000 {
            let amplification_coefficient = rng.gen_range(1..5000);
            let fee_numerator = rng.gen_range(4000000..400000000);

            let amount_a: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let amount_b: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let amount_c: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let curve = Curve {
                amp: amplification_coefficient,
                fee_numerator,
            };

            let admin_fee: u64 = 0; //TODO add test with admin_fee when it is ready
            let amounts = vec![amount_a, amount_b, amount_c];
            let simulation = Simulation::new(
                amplification_coefficient,
                fee_numerator,
                admin_fee,
                amounts.clone(),
                N_COINS as u8,
                None,
                &precision_multipliers,
            );

            check_d(&simulation, &curve, &amounts);

            let i = rng.gen_range(0..N_COINS);
            let j = (i + 1) % N_COINS;
            let amount_x: u128 = rng.gen_range(0..amount_a);

            check_y(
                &simulation,
                &curve,
                i.try_into().unwrap(),
                j.try_into().unwrap(),
                amount_x.into(),
                &amounts,
            );

            check_y_d(
                &simulation,
                &curve,
                i.try_into().unwrap(),
                amount_x.into(),
                &amounts,
            );
        }
    }

    #[test]
    fn test_check_get_withdrawal_amount_for_removing_one_token() {
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let amount_a: u128 = 1511922780182839;
        let amount_b: u128 = 1932395124083698;
        let amount_c: u128 = 1224082092237124;
        let lp_total_supply: u64 = 411922780182839;
        let unmint_amount: u64 = 175849489;
        let amounts = vec![amount_a, amount_b, amount_c];

        let result = curve
            .remove_liquidity_single_token(&amounts, unmint_amount, 2, lp_total_supply)
            .unwrap();

        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let precision_multipliers = vec![1, 1, 1];
        let simulation = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            amounts,
            N_COINS as u8,
            Some(lp_total_supply),
            &precision_multipliers,
        );
        assert_eq!(
            result,
            simulation.sim_calc_withdraw_one_coin(unmint_amount, 2)
        );
    }

    fn check_adding_liquidity(
        curve: &Curve,
        old_balances: &[u128],
        new_balances: &[u128],
        token_supply: u64,
        precision_multipliers: &[u64],
    ) -> u64 {
        println!("{:?} {:?}", old_balances, new_balances);
        let result = curve
            .deposit(old_balances, new_balances, token_supply)
            .unwrap();

        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let simulation = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            old_balances.to_vec(),
            N_COINS as u8,
            Some(token_supply),
            &precision_multipliers,
        );
        let mut amounts = Vec::with_capacity(old_balances.len());
        for i in 0..N_COINS {
            let sub_amount = new_balances[i] - old_balances[i];
            amounts.push(sub_amount)
        }

        let simulation_result = simulation.sim_add_liquidity(amounts);

        // The simulation results can be off by 1 sometimes. It is small enough that we can ignore it.
        if result > simulation_result {
            assert!((result - simulation_result) <= 1);
        } else {
            assert!((simulation_result - result) <= 1);
        }
        result
    }

    fn check_exchange(curve: Curve, i: u8, j: u8, in_amount: u64, balances: &[u128]) {
        println!("{}", in_amount);
        for balance in balances.iter() {
            println!("balance {}", balance);
        }
        let result = curve
            .exchange(
                i.try_into().ok().unwrap(),
                j.try_into().ok().unwrap(),
                in_amount,
                balances,
            )
            .unwrap();
        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let precision_multipliers = vec![1u64, 1u64, 1u64];
        let simulation = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            vec![balances[0], balances[1], balances[2]],
            N_COINS as u8,
            None,
            &precision_multipliers,
        );

        assert_eq!(result, simulation.exchange(i, j, in_amount));
    }

    #[test]
    fn test_exchange_with_0() {
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let in_amount: u64 = 0;
        let amount_a: u128 = 1_000_000;
        // The 0 to 1 bug will only happen when amount_b is a lot more than amount_a
        let amount_b: u128 = 8_000_000;
        let amount_c: u128 = 8_000_000;

        check_exchange(
            curve,
            0, // token a
            1, // token b
            in_amount,
            &vec![amount_a, amount_b, amount_c],
        );
    }

    #[test]
    fn test_get_exchange() {
        let precision_parameters = vec![(6u64, vec![1u64, 1u64, 1u64])];

        for p in precision_parameters {
            check_get_exchange(p.0, &p.1);
        }
    }

    fn check_get_exchange(precision_factor: u64, precision_multipliers: &[u64]) {
        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let admin_fee: u64 = 0; // TODO: add test with admin_fee when it is ready
        let simulation_no_balance = Simulation::new(
            AMPLIFICATION_COEFFICIENT,
            TRADING_FEE,
            admin_fee,
            vec![0, 0, 0, 0],
            N_COINS as u8,
            None,
            &precision_multipliers,
        );
        check_d(
            &simulation_no_balance,
            &curve,
            &[0u128, 0u128, 0u128].to_vec(),
        );

        let amount_a: u128 = 1511922780182839;
        let amount_b: u128 = 1932395124083698;
        let amount_c: u128 = 1724082092237124;
        let amounts = vec![amount_a, amount_b, amount_c];
        let in_amount: u64 = 456739867162;
        check_exchange(curve, 0, 1, in_amount.into(), &amounts);
    }

    #[test]
    fn test_exchange_with_random_inputs() {
        let mut rng: StdRng = rand::SeedableRng::seed_from_u64(SEED);
        for _ in 0..100 {
            let curve = Curve {
                amp: AMPLIFICATION_COEFFICIENT,
                fee_numerator: TRADING_FEE,
            };
            // Max ~ 2 billion USD for each amount, to be plenty
            let amount_a: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let amount_b: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let amount_c: u128 = rng.gen_range(100000000..u128::MAX / 10u128.pow(20));
            let amounts = vec![amount_a, amount_b, amount_c];
            let i = rng.gen_range(0..N_COINS as u8);
            let j = (i + 1) % (N_COINS as u8);
            // The maximum in_amount is 1/1000 of max balance, prevents failure
            let in_amount: u64 = rng.gen_range(0..u64::MAX / 10u64.pow(7));
            check_exchange(curve, i, j, in_amount.into(), &amounts);
        }
    }

    #[test]
    fn test_get_virtual_price() {
        let precision_factor = 6u64;

        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let balances = vec![1000000000000, 1000000000000, 1000000000000];
        let lp_token_total = curve.deposit(&vec![0, 0, 0], &balances, 0);
        let virtual_price = curve.get_virtual_price(
            &balances,
            lp_token_total.unwrap(),
            precision_factor.try_into().ok().unwrap(),
        );

        assert_eq!(virtual_price.unwrap(), 10u64.pow(precision_factor as u32));
    }

    #[test]
    fn test_get_virtual_price_with_precision_multipliers() {
        let precision_factor = 9u64;

        let curve = Curve {
            amp: AMPLIFICATION_COEFFICIENT,
            fee_numerator: TRADING_FEE,
        };
        let balances = vec![1000000_000000, 10000000_00000, 1000000_000000000];
        let lp_token_total = curve.deposit(&vec![0, 0, 0], &balances, 0);
        let virtual_price = curve.get_virtual_price(
            &balances,
            lp_token_total.unwrap(),
            precision_factor.try_into().ok().unwrap(),
        );

        assert_eq!(virtual_price.unwrap(), 10u64.pow(precision_factor as u32));
    }

    proptest! {
        #[test]
        fn test_curve_exchange(
            balances in [(100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128)],
            i in 0..3u8,
            j in 0..3u8,
            in_amount in 100..1_000_000_000_000_000u64,
            n_coins in 2..4u8,
            amplification_coefficient in 1..6000u64
        ) {
            prop_assume!(i < n_coins);
            prop_assume!(j < n_coins);
            prop_assume!(i != j);
            prop_assume!(in_amount < balances[i as usize].try_into().ok().unwrap());

            let fee_numerator = 0u64;
            let admin_fee: u64 = 0;
            let precision_multipliers = vec![1u64, 1u64, 1u64, 1u64];
            let curve = Curve {
                amp: amplification_coefficient,
                fee_numerator: 0,
            };

            let balances = balances[..n_coins as usize].to_vec();
            let result = curve.exchange(i.try_into().ok().unwrap(), j.try_into().ok().unwrap(), in_amount, &balances.clone()).unwrap();

            let simulation = Simulation::new(
                amplification_coefficient,
                fee_numerator,
                admin_fee,
                balances.clone(),
                n_coins,
                None,
                &precision_multipliers,
            );
            let sim_result = simulation.exchange(i.into(), j.into(), in_amount.into());
            let sim_result: u64 = sim_result.try_into().ok().unwrap();

            assert!(
                result == sim_result.try_into().ok().unwrap(),
                "result={}, sim_result={}, n_coins={}, amplification_coefficient={}, fee_numerator={}, balances={:?}",
                result,
                sim_result,
                n_coins,
                amplification_coefficient,
                fee_numerator,
                balances
            );
        }

        #[test]
        fn test_curve_get_mint_amount_for_adding_liquidity(
            old_balances in [(100..1_000_000_000_000_000u128), (100..1_000_000_000_000_000u128), (100..1_000_000_000_000_000u128), (100..1_000_000_000_000_000u128)],
            deposit_amounts in [(100..100_000_000_000_000u128), (100..100_000_000_000_000u128), (100..100_000_000_000_000u128), (100..100_000_000_000_000u128)],
            lp_token_total in 100..1_000_000_000_000_000u64,
            n_coins in 2..4u8,
            amplification_coefficient in 1..6000u64
        ) {
            let old_balances = old_balances[..n_coins as usize].to_vec();
            let new_balances = old_balances
                .iter()
                .enumerate()
                .map(|(i, balance)| balance.checked_add(deposit_amounts[i]).unwrap()).collect::<Vec<_>>();

            let fee_numerator = 0u64;
            let admin_fee: u64 = 0;
            let precision_multipliers = vec![1u64, 1u64, 1u64, 1u64];
            let curve = Curve {
                amp: amplification_coefficient,
                fee_numerator: 0,
            };

            let result = curve.deposit(
                &old_balances,
                &new_balances,
                lp_token_total
            ).unwrap();

            let simulation = Simulation::new(
                amplification_coefficient,
                fee_numerator,
                admin_fee,
                old_balances.clone(),
                n_coins,
                Some(lp_token_total),
                &precision_multipliers,
            );
            let sim_result = simulation.sim_add_liquidity(
                deposit_amounts.into()
            );

            assert!(
                result == sim_result,
                "result={}, sim_result={}, n_coins={}, amplification_coefficient={}, fee_numerator={}, old_balances={:?}, deposit_amounts={:?}",
                result,
                sim_result,
                n_coins,
                amplification_coefficient,
                fee_numerator,
                old_balances,
                deposit_amounts,
            );
        }

        #[test]
        fn test_curve_get_withdrawal_amounts_for_removing_liquidity(
            old_balances in [(100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128)],
            unmint_amount in 100..1_000_000_000_000_000u64,
            lp_token_total in 100..1_000_000_000_000_000u64,
            n_coins in 2..4u8,
            amplification_coefficient in 1..6000u64
        ) {
            prop_assume!(unmint_amount <= lp_token_total);

            let old_balances = old_balances[..n_coins as usize].to_vec();
            let fee_numerator = 0u64;
            let admin_fee: u64 = 0;

            let result = Curve::remove_balanced_liquidity(
                &old_balances,
                unmint_amount,
                lp_token_total
            ).unwrap();

            let simulation = Simulation::new(
                amplification_coefficient,
                fee_numerator,
                admin_fee,
                old_balances.clone(),
                n_coins,
                Some(lp_token_total),
                &vec![1u64, 1u64, 1u64, 1u64],
            );
            let sim_result = simulation.sim_remove_liquidity(
                unmint_amount.into()
            );

            assert!(
                result == sim_result,
                "result={:?}, sim_result={:?}, n_coins={}, amplification_coefficient={}, fee_numerator={}, old_balances={:?}, unmint_amount={}",
                result,
                sim_result,
                n_coins,
                amplification_coefficient,
                fee_numerator,
                old_balances,
                unmint_amount,
            );
        }

        #[test]
        fn test_curve_get_withdrawal_amount_for_removing_one_token(
            balances in [(100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128), (100..10_000_000_000_000_000u128)],
            unmint_amount in 100..1_000_000_000_000_000u64,
            lp_token_total in 100..1_000_000_000_000_000u64,
            i in 0..3u8,
            n_coins in 2..4u8,
            amplification_coefficient in 1..6000u64
        ) {
            prop_assume!(n_coins > i);
            prop_assume!(unmint_amount <= lp_token_total);

            let balances = balances[..n_coins as usize].to_vec();

            let fee_numerator = 0u64;
            let admin_fee: u64 = 0;
            let precision_multipliers = vec![1u64, 1u64, 1u64, 1u64];
            let curve = Curve {
                amp: amplification_coefficient,
                fee_numerator: 0,
            };

            let result = curve.remove_liquidity_single_token(
                &balances,
                unmint_amount,
                i,
                lp_token_total
            ).unwrap();

            let simulation = Simulation::new(
                amplification_coefficient,
                fee_numerator,
                admin_fee,
                balances.clone(),
                n_coins,
                Some(lp_token_total),
                &precision_multipliers
            );
            let sim_result = simulation.sim_calc_withdraw_one_coin(
                unmint_amount.into(),
                i.into(),
            );

            assert!(
                result == sim_result.try_into().ok().unwrap(),
                "result={}, sim_result={}, n_coins={}, amplification_coefficient={}, fee_numerator={}, balances={:?}, unmint_amount={}",
                result,
                sim_result,
                n_coins,
                amplification_coefficient,
                fee_numerator,
                balances,
                unmint_amount,
            );
        }
    }
}
