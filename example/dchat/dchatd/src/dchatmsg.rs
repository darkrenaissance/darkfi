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

// ANCHOR: msg
use smol::lock::Mutex;
use std::sync::Arc;

use darkfi::{impl_p2p_message, net::Message};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

pub type DchatMsgsBuffer = Arc<Mutex<Vec<DchatMsg>>>;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DchatMsg {
    pub msg: String,
}

impl_p2p_message!(DchatMsg, "DchatMsg");
// ANCHOR_END: msg
