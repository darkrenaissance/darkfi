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

use std::{io::Cursor, str::SplitAsciiWhitespace, sync::Arc, time::UNIX_EPOCH};

use darkfi::{
    event_graph::Event,
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
use darkfi_serial::serialize_async;
use smol::lock::RwLock;

use super::super::{client::ReplyType, rpl::*};
use crate::{
    crypto::rln::{closest_epoch, RlnIdentity, RLN2_REGISTER_ZKBIN},
    IrcServer,
};

pub const ACCOUNTS_DB_PREFIX: &str = "darkirc_account_";
pub const ACCOUNTS_KEY_RLN_IDENTITY: &[u8] = b"rln_identity";

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
    pub _username: Arc<RwLock<String>>,
    /// Client nickname
    pub nickname: Arc<RwLock<String>>,
    /// Pointer to parent `IrcServer`
    pub server: Arc<IrcServer>,
}

impl NickServ {
    /// Instantiate a new `NickServ` for a client. This should be called after
    /// the user/nick are successfully registered.
    pub async fn new(
        _username: Arc<RwLock<String>>,
        nickname: Arc<RwLock<String>>,
        server: Arc<IrcServer>,
    ) -> Result<Self> {
        Ok(Self { _username, nickname, server })
    }

    /// Handle a `NickServ` query. This is the main command handler.
    /// Called from `command::handle_cmd_privmsg`.
    pub async fn handle_query(&self, query: &str) -> Result<Vec<ReplyType>> {
        let nick = self.nickname.read().await.to_string();
        let mut tokens = query.split_ascii_whitespace();

        tokens.next();
        let Some(command) = tokens.next() else {
            return Ok(vec![ReplyType::Server((
                ERR_NOTEXTTOSEND,
                format!("{nick} :No text to send"),
            ))])
        };
        let command = command.strip_prefix(':').unwrap();

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
        // Gather the tokens
        let account_name = tokens.next();
        let identity_nullifier = tokens.next();
        let identity_trapdoor = tokens.next();
        let user_msg_limit = tokens.next();

        if account_name.is_none() ||
            identity_nullifier.is_none() ||
            identity_trapdoor.is_none() ||
            user_msg_limit.is_none()
        {
            return Ok(vec![
                ReplyType::Notice((
                    "NickServ".to_string(),
                    nick.to_string(),
                    "Invalid syntax.".to_string(),
                )),
                ReplyType::Notice((
                    "NickServ".to_string(),
                    nick.to_string(),
                    "Use `REGISTER <account_name> <identity_nullifier> <identity_trapdoor> <user_msg_limit>`."
                        .to_string(),
                )),
            ])
        };

        let account_name = account_name.unwrap();
        let identity_nullifier = identity_nullifier.unwrap();
        let identity_trapdoor = identity_trapdoor.unwrap();
        let user_msg_limit: u64 =
            user_msg_limit.unwrap().parse().expect("msg limit must be a number");

        // Open the sled tree
        let db =
            self.server.darkirc.sled.open_tree(format!("{ACCOUNTS_DB_PREFIX}{account_name}"))?;

        if !db.is_empty() {
            return Ok(vec![ReplyType::Notice((
                "NickServ".to_string(),
                nick.to_string(),
                "This account name is already registered.".to_string(),
            ))])
        }

        // Open the sled tree
        let db_default =
            self.server.darkirc.sled.open_tree(format!("{}default", ACCOUNTS_DB_PREFIX))?;

        // Parse the secrets
        let nullifier_bytes = bs58::decode(identity_nullifier).into_vec()?;
        let identity_nullifier =
            match pallas::Base::from_repr(nullifier_bytes.try_into().unwrap()).into_option() {
                Some(v) => v,
                None => {
                    return Ok(vec![ReplyType::Notice((
                        "NickServ".to_string(),
                        nick.to_string(),
                        format!("Invalid identity_nullifier"),
                    ))])
                }
            };

        let trapdoor_bytes = bs58::decode(identity_trapdoor).into_vec()?;
        let identity_trapdoor =
            match pallas::Base::from_repr(trapdoor_bytes.try_into().unwrap()).into_option() {
                Some(v) => v,
                None => {
                    return Ok(vec![ReplyType::Notice((
                        "NickServ".to_string(),
                        nick.to_string(),
                        format!("Invalid identity_trapdoor"),
                    ))])
                }
            };

        // Create a new RLN identity and insert it into the db tree
        let new_rln_identity = RlnIdentity {
            nullifier: identity_nullifier,
            trapdoor: identity_trapdoor,
            user_message_limit: user_msg_limit,
            message_id: 0, // TODO
            last_epoch: closest_epoch(UNIX_EPOCH.elapsed().unwrap().as_millis() as u64),
        };

        // Store account
        db.insert(ACCOUNTS_KEY_RLN_IDENTITY, serialize_async(&new_rln_identity).await)?;
        // Set default account if not already
        if db_default.is_empty() {
            db_default
                .insert(ACCOUNTS_KEY_RLN_IDENTITY, serialize_async(&new_rln_identity).await)?;
        }

        *self.server.rln_identity.write().await = Some(new_rln_identity);

        // Update SMT, DAG and broadcast
        let rln_commitment = new_rln_identity.commitment();
        let evgr = &self.server.darkirc.event_graph;
        let event = Event::new_static(serialize_async(&rln_commitment).await, evgr).await;

        let mut identity_tree = self.server.rln_identity_tree.write().await;

        // Retrieve the register ZK proving key from the db
        let register_zkbin = ZkBinary::decode(RLN2_REGISTER_ZKBIN)?;
        let register_circuit = ZkCircuit::new(empty_witnesses(&register_zkbin)?, &register_zkbin);
        let Some(proving_key) = self.server.server_store.get("rlnv2-diff-register-pk")? else {
            return Err(Error::DatabaseError(
                "RLN register proving key not found in server store".to_string(),
            ))
        };
        let mut reader = Cursor::new(proving_key);
        let proving_key = ProvingKey::read(&mut reader, register_circuit)?;

        let proof =
            new_rln_identity.create_register_proof(&event, &mut identity_tree, &proving_key)?;

        let blob = serialize_async(&(proof, user_msg_limit)).await;

        evgr.static_insert(&event).await?;
        evgr.static_broadcast(event, blob).await?;

        Ok(vec![ReplyType::Notice((
            "NickServ".to_string(),
            nick.to_string(),
            format!("Successfully registered account \"{account_name}\""),
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
        self.server.darkirc.sled.drop_tree(format!("{ACCOUNTS_DB_PREFIX}{account_name}"))?;

        Ok(vec![ReplyType::Notice((
            "NickServ".to_string(),
            nick.to_string(),
            format!("Successfully deregistered account \"{account_name}\""),
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
