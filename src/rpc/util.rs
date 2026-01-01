/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::collections::HashMap;

pub use tinyjson::JsonValue::{
    self, Array as JsonArray, Number as JsonNum, Object as JsonObj, String as JsonStr,
};

// helper functions
pub fn json_map<const N: usize>(vals: [(&str, JsonValue); N]) -> JsonValue {
    JsonObj(HashMap::from(vals.map(|(k, v)| (k.to_string(), v))))
}

pub fn json_str(val: &str) -> JsonValue {
    JsonStr(val.to_string())
}
