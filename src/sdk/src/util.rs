/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use super::error::{ContractError, GenericResult};

pub const GET_SYSTEM_TIME_FAILED: i64 = -1;

/// Everyone can call this. Will return current system timestamp.
///
/// ```
/// timestamp = get_system_time();
/// ```
pub fn get_system_time() -> GenericResult<i64> {
    unsafe {
        let ret = get_system_time_();

        if ret < 0 {
            match ret {
                GET_SYSTEM_TIME_FAILED => return Err(ContractError::GetSystemTimeFailed),
                _ => unimplemented!(),
            }
        }

        Ok(ret)
    }
}

extern "C" {
    fn get_system_time_() -> i64;
}
