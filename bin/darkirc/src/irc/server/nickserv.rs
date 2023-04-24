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

use std::collections::BTreeMap;

use sled::IVec;

use crate::PrivMsgEvent;

#[derive(Debug, Clone, Default)]
pub struct NickServ {
    _db: BTreeMap<IVec, IVec>,
}

impl NickServ {
    fn usage() -> Vec<String> {
        let r = vec![
            "***** nickserv help *****",
            "",
            "nickserv allows clients to 'register' an account. An account",
            "registration is necessary to be able to join and send messages",
            "to the p2p network.",
            "",
            "The following commands are available:",
            "",
            "    CREATE      Create a new account",
            "    LIST        List available accounts",
            "    REGISTER    Register a new account",
            "    IDENTIFY    Identify and pick an account to use",
            "",
            "***** end of help *****",
        ];

        r.iter().map(|x| x.to_string()).collect()
    }

    // Here because we might consider returning the actual full protocol PRIVMSG
    // in a vec. So we can use the result of this and feed it directly to the
    // client as messages. Dunno if necessary, just a thought.
    fn reply(msg: String) -> Vec<String> {
        vec![msg]
    }

    /// Parse an incoming nickserv message
    pub fn act(&mut self, ev: PrivMsgEvent) -> Result<Vec<String>, Vec<String>> {
        assert_eq!(ev.target.to_lowercase().as_str(), super::NICK_NICKSERV);

        let parts: Vec<String> = ev.msg.split(' ').map(|x| x.to_string()).collect();

        match parts[0].to_uppercase().as_str() {
            "CREATE" => self.create(),

            "LIST" => self.list(),

            "REGISTER" => self.register(),

            "IDENTIFY" => self.identify(),

            "HELP" => Ok(Self::usage()),

            c => Err(vec![format!("Invalid command {}", c), "Type HELP to get help".to_string()]),
        }
    }

    /// Create a new account
    fn create(&mut self) -> Result<Vec<String>, Vec<String>> {
        Ok(Self::reply("Account created successfully.".to_string()))
    }

    /// List available accounts
    fn list(&self) -> Result<Vec<String>, Vec<String>> {
        Ok(Self::reply("Accounts: ...".to_string()))
    }

    /// Register a created but unregistered account
    fn register(&mut self) -> Result<Vec<String>, Vec<String>> {
        Ok(Self::reply("Account created successfully.".to_string()))
    }

    /// Pick an account to use
    fn identify(&mut self) -> Result<Vec<String>, Vec<String>> {
        Ok(Self::reply("Using account 0.".to_string()))
    }
}
