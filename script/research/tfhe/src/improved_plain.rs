/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

fn compute_prefix_sum(arr: &[u16]) -> Vec<u16> {
    let mut sum = 0;
    arr.iter()
        .map(|a| {
            sum += a;
            sum
        })
        .collect()
}

fn fill_orders(total_orders: u16, orders: &mut [u16], prefix_sum_arr: &[u16]) {
    for (i, order) in orders.iter_mut().enumerate() {
        let previous_prefix_sum = if i == 0 { 0 } else { prefix_sum_arr[i - 1] };

        *order = (total_orders as i64 - previous_prefix_sum as i64)
            .max(0)
            .min(*order as i64) as u16;
    }
}

pub fn volume_match(sell_orders: &mut [u16], buy_orders: &mut [u16]) {
    let prefix_sum_sell_orders = compute_prefix_sum(sell_orders);

    let prefix_sum_buy_orders = compute_prefix_sum(buy_orders);

    let total_buy_orders = *prefix_sum_buy_orders.last().unwrap_or(&0);

    let total_sell_orders = *prefix_sum_sell_orders.last().unwrap_or(&0);

    fill_orders(total_sell_orders, buy_orders, &prefix_sum_buy_orders);
    fill_orders(total_buy_orders, sell_orders, &prefix_sum_sell_orders);
}
