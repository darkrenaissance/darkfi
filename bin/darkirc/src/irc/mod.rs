/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{collections::HashSet, sync::Arc};

use crypto_box::ChaChaBox;
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

/// IRC client state
pub(crate) mod client;

/// IRC server implementation
pub(crate) mod server;

/// IRC command handler
pub(crate) mod command;

/// IRC numerics and server replies
pub(crate) mod rpl;

/// Hardcoded server name
const SERVER_NAME: &str = "irc.dark.fi";

/// IRC PRIVMSG
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

/// IRC channel definition
#[derive(Clone)]
pub struct IrcChannel {
    pub topic: String,
    pub nicks: HashSet<String>,
    pub saltbox: Option<Arc<ChaChaBox>>,
}

/// IRC contact definition
#[derive(Clone)]
pub struct IrcContact {
    pub saltbox: Option<Arc<ChaChaBox>>,
}
