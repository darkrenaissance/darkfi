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

use darkfi::event_graph::EventMsg;
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[derive(SerialEncodable, SerialDecodable, Clone, Debug)]
pub struct GenEvent {
    pub nick: String,
    pub title: String,
    pub text: String,
}

impl EventMsg for GenEvent {
    fn new() -> Self {
        Self {
            nick: "groot".to_string(),
            title: "I am groot".to_string(),
            text: "I am groot!!".to_string(),
        }
    }
}
