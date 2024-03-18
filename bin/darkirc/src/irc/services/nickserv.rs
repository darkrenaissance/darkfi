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

use std::sync::Arc;

use darkfi::Result;
use smol::lock::RwLock;

use super::super::{client::ReplyType, rpl::*};
use crate::IrcServer;

const NICKSERV_USAGE: &str = r#"***** NickServ Help ***** 

NickServ allows a client to perform account management on DarkIRC.

The following commands are available:

  REGISTER      Register an account

For more information on a NickServ command, type:
/msg NickServ HELP <command>

***** End of Help *****
"#;

/// NickServ implementation used for IRC account management
pub struct NickServ {
    /// Client username
    pub username: Arc<RwLock<String>>,
    /// Client nickname
    pub nickname: Arc<RwLock<String>>,
    /// Pointer to parent `IrcServer`
    pub server: Arc<IrcServer>,
}

impl NickServ {
    /// Instantiate a new `NickServ` for a client.
    /// This is called from `Client::new()`
    pub fn new(
        username: Arc<RwLock<String>>,
        nickname: Arc<RwLock<String>>,
        server: Arc<IrcServer>,
    ) -> Self {
        Self { username, nickname, server }
    }

    /// Handle a `NickServ` query. This is the main command handler.
    /// Called from `command::handle_cmd_privmsg`.
    pub async fn handle_query(&self, query: &str) -> Result<Vec<ReplyType>> {
        let nick = self.nickname.read().await.to_string();
        let mut tokens = query.split_ascii_whitespace();

        let Some(command) = tokens.next() else {
            return Ok(vec![ReplyType::Server((
                ERR_NOTEXTTOSEND,
                format!("{} :No text to send", nick),
            ))])
        };

        match command.to_uppercase().as_str() {
            "HELP" => self.reply_help(&nick).await,
            _x => todo!(),
        }
    }

    /// Reply to the HELP command
    pub async fn reply_help(&self, nick: &str) -> Result<Vec<ReplyType>> {
        let replies = NICKSERV_USAGE
            .lines()
            .map(|x| ReplyType::Notice(("NickServ".to_string(), nick.to_string(), x.to_string())))
            .collect();

        Ok(replies)
    }
}
