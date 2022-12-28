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

// ANCHOR: msg
use async_std::sync::{Arc, Mutex};

use darkfi::net;
use darkfi_serial::{SerialDecodable, SerialEncodable};

pub type DchatMsgsBuffer = Arc<Mutex<Vec<DchatMsg>>>;

impl net::Message for DchatMsg {
    fn name() -> &'static str {
        "DchatMsg"
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DchatMsg {
    pub msg: String,
}
// ANCHOR_END: msg
