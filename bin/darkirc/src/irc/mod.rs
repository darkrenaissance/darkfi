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

use std::{collections::HashSet, sync::Arc};

use crypto_box::ChaChaBox;

/// IRC client state
pub mod client;

/// IRC server implementation
pub mod server;

/// IRC command handler
pub mod command;

/// Services implementations
pub mod services;
pub use services::nickserv::NickServ;

/// IRC numerics and server replies
pub mod rpl;

/// Hardcoded server name
const SERVER_NAME: &str = "irc.dark.fi";

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
    /// Saltbox created for our contact public key
    pub saltbox: Arc<ChaChaBox>,
    /// Saltbox used to encrypt our nick in direct messages,
    /// created for our own public key.
    pub self_saltbox: Arc<ChaChaBox>,
}
