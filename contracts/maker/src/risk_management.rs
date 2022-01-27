use crate::{
    state::State,
    utils::{div_dec, sub_abs, sub_no_overflow}, msg::WrappedPosition,
};
use cosmwasm_std::Decimal256 as Decimal;
use std::str::FromStr;

// TODO: add more 
pub fn sanity_check(is_deriv: bool, position: &Option<WrappedPosition>, inv_base_bal: Decimal, state: &State) {
    assert_eq!(is_deriv, state.is_deriv);
    assert!(is_deriv && inv_base_bal == Decimal::zero());
    assert!(!is_deriv || position.is_none());
}

/// Determines the notional balance that we are willing to assign to either the buy/sell side.
/// Takes into consideration the current margin to limit the new open orders on the side
/// that already has a positon open.
/// # Arguments
/// * `inv_val` - The total notional value of the inventory
/// * `margin` - The margin value of an open position (is zero if the position is on the opposite side or if there isn't one)
/// * `active_capital_perct` - The factor by which we multiply the inventory val to get total capital that should be on the book
/// # Returns
/// * `alloc_bal` - The notional balance we are willing to allocate to one side
pub fn get_alloc_bal_new_orders(inv_val: Decimal, margin: Decimal, active_capital_perct: Decimal) -> Decimal {
    let alloc_for_both_sides = inv_val * active_capital_perct;
    let alloc_one_side = div_dec(alloc_for_both_sides, Decimal::from_str("2").unwrap());

    if margin == Decimal::zero() {
        alloc_one_side
    } else {
        let inv_val = sub_no_overflow(inv_val, alloc_one_side);
        let inv_val = sub_no_overflow(inv_val, margin);
        let alloc_for_both_sides = inv_val * active_capital_perct;
        div_dec(alloc_for_both_sides, Decimal::from_str("2").unwrap())
    }
}

/// Ensures that the current tails have enough distance between them. We don't want our order spread to be too dense.
/// If they fall below the minimum distance, we update the tail to something more suitable.
/// # Arguments
/// * `buy_head` - The buy head that we are going to use
/// * `sell_head` - The the sell head that we are going to use
/// * `proposed_buy_tail` - The buyside tail obtained from the mid price
/// * `proposed_sell_tail` - The sellside tail obtained from the mid price
/// * `min_tail_dist_perct` - The minimum distance in from the head that we are willing to tolerate
/// # Returns
/// * `buy_tail` - The new buyside tail post risk management
/// * `sell_tail` - The new sellside tail post risk management
pub fn check_tail_dist(
    buy_head: Decimal,
    sell_head: Decimal,
    proposed_buy_tail: Decimal,
    proposed_sell_tail: Decimal,
    min_tail_dist_perct: Decimal,
) -> (Decimal, Decimal) {
    let buy_tail = if buy_head > proposed_buy_tail {
        let proposed_buy_tail_dist_perct = div_dec(sub_abs(buy_head, proposed_buy_tail), buy_head);
        if proposed_buy_tail_dist_perct < min_tail_dist_perct {
            buy_head * sub_abs(Decimal::one(), min_tail_dist_perct)
        } else {
            proposed_buy_tail
        }
    } else {
        proposed_buy_tail
    };

    let sell_tail = if sell_head < proposed_sell_tail {
        let proposed_sell_tail_dist_perct = div_dec(sub_abs(sell_head, proposed_sell_tail), sell_head);
        if proposed_sell_tail_dist_perct < min_tail_dist_perct {
            sell_head * (Decimal::one() + min_tail_dist_perct)
        } else {
            proposed_sell_tail
        }
    } else {
        proposed_sell_tail
    };

    (buy_tail, sell_tail)
}

/// Ensures that the variance will never be smaller than the std deviation.
/// # Arguments
/// * `std_dev` - The standard deviation
/// # Returns
/// * `safe_variance` - The variance
pub fn safe_variance(mut std_dev: Decimal) -> Decimal {
    let mut shift = Decimal::one();
    let multiplier = Decimal::from_str("10").unwrap();
    while std_dev * std_dev < std_dev {
        std_dev = std_dev * multiplier;
        shift = shift * multiplier;
    }
    div_dec(std_dev * std_dev, shift)
}

#[cfg(test)]
mod tests {
    use super::{check_tail_dist, get_alloc_bal_new_orders, safe_variance};
    use cosmwasm_std::Decimal256 as Decimal;
    use std::str::FromStr;

    #[test]
    fn safe_variance_test() {
        let std_dev = Decimal::from_str("2").unwrap();
        let variance = safe_variance(std_dev);
        assert_eq!(variance, Decimal::from_str("4").unwrap());

        let std_dev = Decimal::from_str("0.4").unwrap();
        let variance = safe_variance(std_dev);
        assert_eq!(variance, Decimal::from_str("1.6").unwrap());
    }

    #[test]
    fn get_alloc_bal_new_orders_test() {
        let inv_val = Decimal::from_str("100000").unwrap();
        let active_capital_perct = Decimal::from_str("0.2").unwrap();
        let margin = Decimal::zero();

        let alloc_bal_a = get_alloc_bal_new_orders(inv_val, margin, active_capital_perct);
        let alloc_bal_b = get_alloc_bal_new_orders(inv_val, margin, active_capital_perct);
        assert_eq!(alloc_bal_a, alloc_bal_b);
        assert_eq!(alloc_bal_a, Decimal::from_str("0.1").unwrap() * inv_val);

        let active_capital_perct = Decimal::from_str("0.1").unwrap();
        let alloc_bal_a = get_alloc_bal_new_orders(inv_val, margin, active_capital_perct);
        let margin = Decimal::from_str("5000").unwrap();
        let alloc_bal_b = get_alloc_bal_new_orders(inv_val, margin, active_capital_perct);
        assert_eq!(alloc_bal_a, Decimal::from_str("5000").unwrap());
        assert_eq!(alloc_bal_b, Decimal::from_str("4500").unwrap());
    }

    #[test]
    fn check_tail_dist_test() {
        let buy_head = Decimal::from_str("3999").unwrap();
        let sell_head = Decimal::from_str("4001").unwrap();
        let proposed_buy_tail = Decimal::from_str("2000").unwrap();
        let proposed_sell_tail = Decimal::from_str("7000").unwrap();
        let min_tail_dist_perct = Decimal::from_str("0.01").unwrap();
        let (buy_tail, sell_tail) = check_tail_dist(buy_head, sell_head, proposed_buy_tail, proposed_sell_tail, min_tail_dist_perct);
        assert_eq!(buy_tail, proposed_buy_tail);
        assert_eq!(sell_tail, proposed_sell_tail);

        let proposed_buy_tail = Decimal::from_str("3998").unwrap();
        let proposed_sell_tail = Decimal::from_str("4002").unwrap();
        let (buy_tail, sell_tail) = check_tail_dist(buy_head, sell_head, proposed_buy_tail, proposed_sell_tail, min_tail_dist_perct);
        assert_eq!(buy_tail, buy_head * (Decimal::one() - min_tail_dist_perct));
        assert_eq!(sell_tail, sell_head * (Decimal::one() + min_tail_dist_perct));
    }
}
