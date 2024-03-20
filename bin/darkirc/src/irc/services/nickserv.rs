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

use std::{str::SplitAsciiWhitespace, sync::Arc};

use darkfi::Result;
use darkfi_sdk::crypto::SecretKey;
use darkfi_serial::serialize_async;
use rand::rngs::OsRng;
use smol::lock::RwLock;

use super::super::{client::ReplyType, rpl::*};
use crate::IrcServer;

const ACCOUNTS_DB_PREFIX: &str = "darkirc_account_";
const ACCOUNTS_KEY_SECRET: &[u8] = b"secret_key";

const NICKSERV_USAGE: &str = r#"***** NickServ Help ***** 

NickServ allows a client to perform account management on DarkIRC.

The following commands are available:

  INFO          Displays information on registrations.
  REGISTER      Register an account.
  DEREGISTER    Deregister an account.
  SET           Select an account to use.

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
    /// Instantiate a new `NickServ` for a client. This should be called after
    /// the user/nick are successfully registered.
    pub async fn new(
        username: Arc<RwLock<String>>,
        nickname: Arc<RwLock<String>>,
        server: Arc<IrcServer>,
    ) -> Result<Self> {
        Ok(Self { username, nickname, server })
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
            "INFO" => self.handle_info(&nick, &mut tokens).await,
            "REGISTER" => self.handle_register(&nick, &mut tokens).await,
            "DEREGISTER" => self.handle_deregister(&nick, &mut tokens).await,
            "SET" => self.handle_set(&nick, &mut tokens).await,
            "HELP" => self.handle_help(&nick).await,
            _ => self.handle_invalid(&nick).await,
        }
    }

    /// Handle the INFO command
    pub async fn handle_info(
        &self,
        _nick: &str,
        _tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        todo!()
    }

    /// Handle the REGISTER command
    pub async fn handle_register(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        let Some(account_name) = tokens.next() else {
            return Ok(vec![ReplyType::Notice((
                "NickServ".to_string(),
                nick.to_string(),
                "Invalid syntax. Use `REGISTER <account_name>`.".to_string(),
            ))])
        };

        // Open the sled tree
        let db = self
            .server
            .darkirc
            .sled
            .open_tree(format!("{}{}", ACCOUNTS_DB_PREFIX, account_name))?;

        if !db.is_empty() {
            return Ok(vec![ReplyType::Notice((
                "NickServ".to_string(),
                nick.to_string(),
                "This account is already registered.".to_string(),
            ))])
        }

        // Create a new secret key and insert it into the db.
        let secret = SecretKey::random(&mut OsRng);
        db.insert(ACCOUNTS_KEY_SECRET, serialize_async(&secret).await)?;

        Ok(vec![ReplyType::Notice((
            "NickServ".to_string(),
            nick.to_string(),
            format!("Successfully registered account \"{}\"", account_name),
        ))])
    }

    /// Handle the DEREGISTER command
    pub async fn handle_deregister(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        let Some(account_name) = tokens.next() else {
            return Ok(vec![ReplyType::Notice((
                "NickServ".to_string(),
                nick.to_string(),
                "Invalid syntax. Use `DEREGISTER <account_name>`.".to_string(),
            ))])
        };

        // Drop the tree
        self.server.darkirc.sled.drop_tree(format!("{}{}", ACCOUNTS_DB_PREFIX, account_name))?;

        Ok(vec![ReplyType::Notice((
            "NickServ".to_string(),
            nick.to_string(),
            format!("Successfully deregistered account \"{}\"", account_name),
        ))])
    }

    /// Handle the SET command
    pub async fn handle_set(
        &self,
        _nick: &str,
        _tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        todo!()
    }

    /// Reply to the HELP command
    pub async fn handle_help(&self, nick: &str) -> Result<Vec<ReplyType>> {
        let replies = NICKSERV_USAGE
            .lines()
            .map(|x| ReplyType::Notice(("NickServ".to_string(), nick.to_string(), x.to_string())))
            .collect();

        Ok(replies)
    }

    /// Reply to an invalid command
    pub async fn handle_invalid(&self, nick: &str) -> Result<Vec<ReplyType>> {
        let replies = vec![
            ReplyType::Notice((
                "NickServ".to_string(),
                nick.to_string(),
                "Invalid NickServ command.".to_string(),
            )),
            ReplyType::Notice((
                "NickServ".to_string(),
                nick.to_string(),
                "Use /msg NickServ HELP for a NickServ command listing.".to_string(),
            )),
        ];

        Ok(replies)
    }
}
