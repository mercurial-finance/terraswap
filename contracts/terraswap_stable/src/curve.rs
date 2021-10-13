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
        let ann_mul_sum_x = (ann as u128).checked_mul(sum_x)?;
        let ann_sub_one = (ann as u128).checked_sub(1)?;

        let mut d_prev: u128;
        let mut d = d_suggest.unwrap_or(sum_x.into());
        for i in 0..ITERATIONS {
            let d_u256 = d;
            let mut d_prod = d_u256;
            for amount_time_coin in amounts_times_coin.iter() {
                d_prod = d_prod.checked_mul(d_u256)?.checked_div(*amount_time_coin)?;
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

        let mut c = d;
        let mut s = 0u128;

        for k in 0..n_coins {
            let x_temp = if k != i as usize {
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

        let mut c = d;
        let mut s: u128 = 0u128;

        for (k, balance) in balances.iter().enumerate() {
            let x_temp = if k == i as usize {
                x
            } else if k != j as usize {
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
            let dx_expected = if j == i as usize {
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
