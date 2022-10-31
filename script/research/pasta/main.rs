/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use pasta_curves as pasta;
use group::{Group, Curve};
use rand::rngs::OsRng;

fn main() {
    let g = pasta::vesta::Point::generator();
    println!("G = {:?}", g.to_affine());
    let x = pasta::vesta::Scalar::from(87u64);
    println!("x = 87 = {:?}", x);
    let b = g * x;
    println!("B = xG = {:?}", b.to_affine());

    let y = x - pasta::vesta::Scalar::from(90u64);
    println!("y = x - 90 = {:?}", y);

    let c = pasta::vesta::Point::random(&mut OsRng);
    let d = pasta::vesta::Point::random(&mut OsRng);
    println!("C = {:?}", c.to_affine());
    println!("D = {:?}", d.to_affine());
    println!("C + D = {:?}", (c + d).to_affine());
}
