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

extern crate darkfi_serial;
extern crate bitcoin;

use honggfuzz::fuzz;
use darkfi_serial::VarInt;
use bitcoin::VarInt as BTCVarInt;
// use bitcoin::consensus::serialize;
// use bitcoin::psbt::serialize;
// use darkfi_serial::{serialize, deserialize};

fn main() {
    loop {
        fuzz!(|data: u64 | {
            let dark_vi = VarInt(data.clone());
            let btc_vi = BTCVarInt(data.clone());
            assert_eq!(
                dark_vi.length(),
                btc_vi.len(),
            );

            let dark_ser = darkfi_serial::serialize(&dark_vi);
            let btc_ser = bitcoin::consensus::serialize(&btc_vi);
            assert_eq!(dark_ser, btc_ser);

            let dark_des: VarInt = darkfi_serial::deserialize(&dark_ser).unwrap();
            let btc_des: BTCVarInt = bitcoin::consensus::deserialize(&btc_ser).unwrap();
            assert_eq!(
                dark_des.length(),
                btc_des.len(),
            );

            // assert_eq!(
            //     darkfi_serial::decode(&dark_ser).unwrap(),
            //     bitcoin::consensus::decode(&btc_ser).unwrap(),
            // );
        });
    }
}
