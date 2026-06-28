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

//! NickServ - account management for DarkIRC.
//!
//! Each registered account is one RLN identity stored under its own
//! sled tree, named `darkirc_account_<name>`. A separate sled tree
//! `darkirc_account_default` mirrors whichever identity is currently
//! active; on startup, `IrcServer::new` reads from that tree to
//! populate `IrcServer::rln_identity`, which is the field every
//! outbound signal proof reads from.
//!
//! Commands:
//!
//! - `REGISTER <name> <nullifier> <trapdoor> <user_msg_limit>` -
//!   create a new account, build a registration proof, broadcast it.
//!   If no account was previously active, the new one becomes
//!   active.
//! - `INFO` (no args) - list all registered accounts, mark the
//!   active one with `*`, show each account's RLN commitment.
//! - `INFO <name>` - dump the secrets for `<name>` so they can be
//!   copied to a different machine or a config file. Output
//!   includes a warning that secrets are appearing in scrollback.
//! - `SET <name>` - swap the active identity to `<name>`. The
//!   change is persisted (next restart will load the same one) and
//!   takes effect for the next outbound message.
//! - `DEREGISTER <name>` - drop the account's sled tree. Refuses
//!   if `<name>` is currently active (the user must SET away first
//!   so they don't accidentally orphan their session). Local-only:
//!   the on-network registration is unaffected, so the same
//!   identity can be re-registered locally on another machine
//!   that holds the same secrets.
//! - `SLASH <name> CONFIRM` - permanently burn the account on the
//!   network. Publishes a slash event into the static DAG; once
//!   accepted by peers, the identity is removed from the SMT
//!   network-wide and CANNOT be re-registered. The slash blob
//!   contains the identity_secret_hash in plaintext, so the
//!   secret becomes world-readable on the wire. This is intended
//!   for retiring an account whose secret has leaked, or for
//!   formally giving up an identity. The literal `CONFIRM` token
//!   is required to suppress accidents.
//! - `HELP` - usage.

use std::{str::SplitAsciiWhitespace, sync::Arc};

use darkfi::{
    event_graph::{
        genesis_commits::GENESIS_COMMITMENTS_REPR,
        rln::{create_slash_proof, RLNNode, SlashBlob, GENESIS_USER_MSG_LIMIT},
        Event,
    },
    Result,
};
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
use darkfi_serial::{deserialize_async, serialize_async};
use smol::lock::RwLock;

use super::super::{client::ReplyType, rpl::*};
use crate::{crypto::rln::RlnIdentity, IrcServer};

pub const ACCOUNTS_DB_PREFIX: &str = "darkirc_account_";
pub const ACCOUNTS_KEY_RLN_IDENTITY: &[u8] = b"rln_identity";

/// Name of the sled tree that mirrors the currently-active identity.
/// `IrcServer::new` reads this on startup.
pub const ACCOUNTS_DEFAULT_TREE: &str = "darkirc_account_default";

// /// Outcome of the `static_broadcast` call inside REGISTER. Used to
// /// pick which "what just happened" notice we send back to the user.
// #[derive(PartialEq, Eq)]
// enum BroadcastStatus {
//     /// Local node was synced; broadcast was issued immediately.
//     Sent,
//     /// Local node was unsynced; the (event, blob) pair was queued
//     /// in `IrcServer::pending_static_broadcasts` and a watcher task
//     /// will broadcast it once sync completes.
//     Deferred,
// }

const NICKSERV_USAGE: &str = r#"***** NickServ Help *****

NickServ allows a client to perform account management on DarkIRC.

The following commands are available:

  INFO          Display information on registrations.
  REGISTER      Register an account.
  DEREGISTER    Deregister an account locally.
  SET           Select an account to use.
  SLASH         Permanently retire an account network-wide.

For more information on a NickServ command, type:
/msg NickServ HELP <command>

***** End of Help *****
"#;

const NICKSERV_INFO_HELP: &str = r#"***** NickServ Help: INFO *****

INFO with no arguments lists every registered account, marks the
currently-active one with an asterisk, and shows each account's
RLN commitment (a public identifier).

INFO <account_name> dumps that account's secrets in a form ready to
paste back into REGISTER on another machine. Be aware that this
prints the secrets to your IRC client where they may end up in
scrollback or logs.

  INFO
  INFO <account_name>

***** End of Help *****
"#;

const NICKSERV_REGISTER_HELP: &str = r#"***** NickServ Help: REGISTER *****

REGISTER creates a new RLN identity from the supplied secrets,
broadcasts a registration proof to the network, and stores the
account locally. The first account registered also becomes the
active one.

To generate a fresh nullifier/trapdoor pair, run:

  darkirc --gen-rln-identity

  REGISTER <account_name> <nullifier> <trapdoor> <user_msg_limit>

  account_name      - any local label, e.g. "alice" or "throwaway"
  nullifier         - base58-encoded pallas::Base scalar
  trapdoor          - base58-encoded pallas::Base scalar
  user_msg_limit    - per-epoch message budget; max 10 on the free
                      tier (RegistrationAttestation::FREE_TIER_LIMIT)

***** End of Help *****
"#;

const NICKSERV_SET_HELP: &str = r#"***** NickServ Help: SET *****

SET swaps the active identity to the named account. Outbound
messages from now on use that account's commitment and per-epoch
budget. The choice is persisted across restarts.

If you have used this identity recently from another node, wait
one RLN epoch (10 minutes) before sending - the in-memory message
counter resets on swap, and a clash with another node could cause
a slash.

  SET <account_name>

***** End of Help *****
"#;

const NICKSERV_DEREGISTER_HELP: &str = r#"***** NickServ Help: DEREGISTER *****

DEREGISTER removes an account from local storage. The on-network
RLN registration is permanent and CANNOT be undone by this command;
DEREGISTER only forgets the account locally. If you registered this
identity on the network and care about reusing it, save its INFO
output first.

You cannot DEREGISTER the active account; SET to a different one
first.

If you want to permanently retire the identity NETWORK-WIDE so that
no one (including you) can ever use it again, see SLASH.

  DEREGISTER <account_name>

***** End of Help *****
"#;

const NICKSERV_SLASH_HELP: &str = r#"***** NickServ Help: SLASH *****

SLASH publishes a slash event for the named account into the static
DAG. Once accepted by peers, the identity is removed from the
network's identity tree and CANNOT be re-registered, by you or by
anyone else. The slash blob contains the identity_secret_hash in
plaintext, so any observer of the static DAG can see your secret
after a SLASH - treat the secret as compromised after this command.

Use cases:
  - Your identity's secrets leaked and you want to retire the
    account so an attacker can't impersonate you any longer.
  - You want to formally give up an identity (e.g. before disposing
    of a machine).

This is irreversible and visible to the entire network. The
literal `CONFIRM` token is required to proceed.

You cannot SLASH the active account; SET to a different one first
(if you have one). SLASH refuses while the local DAG is unsynced
because the slash proof must be built against a canonical SMT root
that peers will recognize.

After a successful SLASH the local account tree is also dropped
(equivalent to a DEREGISTER on top of the network slash).

  SLASH <account_name> CONFIRM

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

/// Convenience helper - build a NickServ NOTICE reply.
fn notice(nick: &str, body: impl Into<String>) -> ReplyType {
    ReplyType::Notice(("NickServ".to_string(), nick.to_string(), body.into()))
}

/// Convenience helper - build several NOTICE replies from an iterator
/// of strings, one per line.
fn notices<I, S>(nick: &str, lines: I) -> Vec<ReplyType>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    lines.into_iter().map(|s| notice(nick, s)).collect()
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
            "SLASH" => self.handle_slash(&nick, &mut tokens).await,
            "HELP" => self.handle_help(&nick, &mut tokens).await,
            _ => self.handle_invalid(&nick).await,
        }
    }

    /// Handle the INFO command.
    ///
    /// `INFO` (no args)        -> account list with active marker
    /// `INFO <account_name>`   -> secrets dump for that account
    pub async fn handle_info(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        match tokens.next() {
            None => self.handle_info_list(nick).await,
            Some(account_name) => self.handle_info_account(nick, account_name).await,
        }
    }

    /// `INFO` with no args. Walks every `darkirc_account_*` tree,
    /// loads the identity to recover its commitment, and marks the
    /// one whose commitment matches the in-memory active identity.
    async fn handle_info_list(&self, nick: &str) -> Result<Vec<ReplyType>> {
        // The active identity's commitment is what we compare
        // against. We deliberately do NOT compare account names,
        // because the default tree stores the identity blob, not
        // a name. Comparing commitments means we get the right
        // answer even if the user renamed trees by hand.
        let active_commitment =
            self.server.rln_identity.read().await.as_ref().map(|id| id.commitment());

        let mut accounts: Vec<(String, RlnIdentity)> = Vec::new();
        for raw in self.server.darkirc.sled.tree_names() {
            // `raw` is a sled IVec; coerce to bytes via AsRef so
            // this compiles regardless of which sled fork the
            // workspace pulls in.
            let bytes: &[u8] = raw.as_ref();
            let Ok(name) = std::str::from_utf8(bytes) else { continue };
            // Skip the `default` mirror tree and anything that
            // isn't an account tree. Note we strip the prefix once
            // and reject the literal "default" suffix - we do NOT
            // want to list the mirror as if it were a separate
            // account.
            let Some(account_name) = name.strip_prefix(ACCOUNTS_DB_PREFIX) else { continue };
            if account_name == "default" || account_name.is_empty() {
                continue
            }

            let tree = self.server.darkirc.sled.open_tree(name)?;
            let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else { continue };
            // If a tree exists but the blob is malformed, skip
            // rather than failing the whole listing.
            let Ok(identity): std::result::Result<RlnIdentity, _> = deserialize_async(&blob).await
            else {
                continue
            };

            accounts.push((account_name.to_string(), identity));
        }

        if accounts.is_empty() {
            return Ok(vec![notice(nick, "No registered accounts. Use REGISTER to create one.")])
        }

        // Stable ordering for predictable output.
        accounts.sort_by(|a, b| a.0.cmp(&b.0));

        let mut lines: Vec<String> = Vec::with_capacity(accounts.len() + 2);
        lines.push("Registered accounts (* = active):".to_string());
        for (name, id) in &accounts {
            let active_mark = if Some(id.commitment()) == active_commitment { "*" } else { " " };
            // The commitment is a public value (it lives in the
            // SMT) so showing it here doesn't leak anything.
            let commitment_b58 = bs58::encode(id.commitment().to_repr()).into_string();
            lines.push(format!(
                "  {active_mark} {name}  limit={limit}  commitment={commitment_b58}",
                limit = id.user_message_limit,
            ));
        }
        lines.push(
            "Use `INFO <account_name>` to show that account's secrets (REGISTER args).".to_string(),
        );

        Ok(notices(nick, lines))
    }

    /// `INFO <account_name>`. Dumps the secrets so the user can
    /// reconstruct the identity elsewhere.
    async fn handle_info_account(&self, nick: &str, account_name: &str) -> Result<Vec<ReplyType>> {
        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        if account_name == "default" || account_name.is_empty() {
            return Ok(vec![notice(nick, "Invalid account name.")])
        }

        let tree = self.server.darkirc.sled.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(vec![notice(nick, format!("No such account: \"{account_name}\""))])
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(vec![notice(
                    nick,
                    format!("Account \"{account_name}\" exists but its data is corrupted."),
                )])
            }
        };

        let nullifier_b58 = bs58::encode(identity.nullifier.to_repr()).into_string();
        let trapdoor_b58 = bs58::encode(identity.trapdoor.to_repr()).into_string();
        let commitment_b58 = bs58::encode(identity.commitment().to_repr()).into_string();

        // Active marker.
        let active_commitment =
            self.server.rln_identity.read().await.as_ref().map(|id| id.commitment());
        let is_active = Some(identity.commitment()) == active_commitment;

        let lines = vec![
            format!("Account \"{account_name}\"{}:", if is_active { " (ACTIVE)" } else { "" }),
            format!("  commitment       = {commitment_b58}"),
            format!("  user_msg_limit   = {}", identity.user_message_limit),
            "  --- secrets below; treat as a password ---".to_string(),
            format!("  nullifier        = {nullifier_b58}"),
            format!("  trapdoor         = {trapdoor_b58}"),
            "To re-register on another node, run:".to_string(),
            format!(
                "  /msg NickServ REGISTER {account_name} {nullifier_b58} {trapdoor_b58} {limit}",
                limit = identity.user_message_limit,
            ),
        ];

        Ok(notices(nick, lines))
    }

    /// Handle the REGISTER command.
    ///
    /// `REGISTER <account_name> <nullifier> <trapdoor> <user_msg_limit>`
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
            return Ok(notices(
                nick,
                [
                    "Invalid syntax.",
                    "Use `REGISTER <account_name> <identity_nullifier> \
                     <identity_trapdoor> <user_msg_limit>`.",
                    "Run `darkirc --gen-rln-identity` to mint fresh secrets.",
                ],
            ))
        };

        let account_name = account_name.unwrap();
        let identity_nullifier = identity_nullifier.unwrap();
        let identity_trapdoor = identity_trapdoor.unwrap();

        // Reserved name. We use `default` for the mirror tree.
        if account_name == "default" || account_name.is_empty() {
            return Ok(vec![notice(nick, "Invalid account name.")])
        }

        // Parse user_msg_limit defensively. The original code
        // panicked here, which would tear down the whole IRC
        // session on a typo.
        let user_msg_limit: u64 = match user_msg_limit.unwrap().parse() {
            Ok(v) => v,
            Err(_) => {
                return Ok(vec![notice(nick, "Invalid user_msg_limit: must be a positive integer.")])
            }
        };
        if user_msg_limit == 0 {
            return Ok(vec![notice(nick, "Invalid user_msg_limit: must be at least 1.")])
        }

        // Open the per-account sled tree
        let db =
            self.server.darkirc.sled.open_tree(format!("{ACCOUNTS_DB_PREFIX}{account_name}"))?;

        if !db.is_empty() {
            return Ok(vec![notice(nick, "This account name is already registered.")])
        }

        // Open the default-mirror sled tree
        let db_default = self.server.darkirc.sled.open_tree(ACCOUNTS_DEFAULT_TREE)?;

        // Parse the secrets. The original code used `.unwrap()` on
        // the `try_into` for the byte-length check, which would
        // panic on any input that wasn't exactly 32 bytes. Convert
        // to a graceful error instead.
        let identity_nullifier = match parse_pallas_b58(identity_nullifier) {
            Some(v) => v,
            None => return Ok(vec![notice(nick, "Invalid identity_nullifier.")]),
        };
        let identity_trapdoor = match parse_pallas_b58(identity_trapdoor) {
            Some(v) => v,
            None => return Ok(vec![notice(nick, "Invalid identity_trapdoor.")]),
        };

        // Create a new RLN identity. `last_epoch` is initialised to
        // 0 deterministically - the first call to `next_message_id`
        // will detect the rollover to the current wall-clock epoch.
        let new_rln_identity = RlnIdentity {
            nullifier: identity_nullifier,
            trapdoor: identity_trapdoor,
            user_message_limit: user_msg_limit,
            message_id: 0,
            last_epoch: 0,
        };

        // Store account
        db.insert(ACCOUNTS_KEY_RLN_IDENTITY, serialize_async(&new_rln_identity).await)?;

        // First-ever registration also becomes the active one. We
        // check the in-memory active identity (not the default
        // tree) because that's the source of truth at runtime.
        let became_active = self.server.rln_identity.read().await.is_none();
        if became_active {
            db_default
                .insert(ACCOUNTS_KEY_RLN_IDENTITY, serialize_async(&new_rln_identity).await)?;
            *self.server.rln_identity.write().await = Some(new_rln_identity);
        }

        let is_genesis =
            GENESIS_COMMITMENTS_REPR.contains(&new_rln_identity.commitment().to_repr());

        if is_genesis {
            if user_msg_limit != GENESIS_USER_MSG_LIMIT {
                let mut replies = vec![notice(
                    nick,
                    format!("Genesis account must use user_msg_limit={}", GENESIS_USER_MSG_LIMIT),
                )];
                replies.push(notice(
                    nick,
                    format!("Use `DEREGISTER {account_name}` to remove this account."),
                ));
            }
            let mut replies =
                vec![notice(nick, format!("Successfully registered account \"{account_name}\""))];
            if became_active {
                replies
                    .push(notice(nick, format!("\"{account_name}\" is now the active identity.")));
            } else {
                replies.push(notice(
                    nick,
                    format!("Use `SET {account_name}` to make this the active identity."),
                ));
            }
            Ok(replies)
        } else {
            let replies =
                vec![notice(nick, format!("Failed to register account \"{account_name}\""))];
            Ok(replies)
        }
        // Apply the registration through the canonical pipeline:
        //
        // 1. `apply_rln_static_event` mutates the SMT and records
        //    the post-mutation root in the historical-roots table.
        //    Same entry point that `proto.rs::handle_static_put`
        //    calls for events arriving over the wire, so locally-
        //    originated and remote registrations end up in the
        //    same canonical state.
        // 2. `static_blob_store` persists the blob alongside the
        //    event so a future late-joiner can re-verify the proof
        //    during `static_sync`.
        // 3. `static_insert` writes the event to the static DAG
        //    and notifies `static_pub` (which the IRC client
        //    subscription picks up for its own bookkeeping).
        // 4. `static_broadcast` re-emits to peers - but ONLY if
        //    the local DAG is synced. A pre-sync broadcast just
        //    vanishes (peers gate `handle_static_put` on their own
        //    is_synced state, and we can't serve our tips for
        //    pulls because `handle_tip_req` gates on is_synced
        //    too). When unsynced, we defer the broadcast to a
        //    watcher task that drains the pending queue once sync
        //    completes.
        /*
        evgr.apply_rln_static_event(&event, &rln_node).await?;
        evgr.static_blob_store(&event.id(), &blob_bytes)?;
        evgr.static_insert(&event).await?;

        let broadcast_status = if evgr.is_synced() {
            evgr.static_broadcast(event, blob_bytes).await?;
            BroadcastStatus::Sent
        } else {
            self.server.pending_static_broadcasts.lock().await.push((event, blob_bytes));
            BroadcastStatus::Deferred
        };

        let mut replies =
            vec![notice(nick, format!("Successfully registered account \"{account_name}\""))];
        if became_active {
            replies.push(notice(nick, format!("\"{account_name}\" is now the active identity.")));
        } else {
            replies.push(notice(
                nick,
                format!("Use `SET {account_name}` to make this the active identity."),
            ));
        }

        if broadcast_status == BroadcastStatus::Deferred {
            replies.push(notice(
                nick,
                "Note: local DAG is not yet synced; the registration is stored \
                 locally and will be broadcast to peers once sync completes.",
            ));
        }

        Ok(replies)
        */
    }

    /// Handle the DEREGISTER command.
    ///
    /// Refuses to drop the active account so the user doesn't
    /// orphan their session into a state where outbound messages
    /// would still reference an identity whose tree is gone. The
    /// network-side registration is permanent regardless - this
    /// only clears local state.
    pub async fn handle_deregister(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        let Some(account_name) = tokens.next() else {
            return Ok(vec![notice(nick, "Invalid syntax. Use `DEREGISTER <account_name>`.")])
        };
        if account_name == "default" || account_name.is_empty() {
            return Ok(vec![notice(nick, "Invalid account name.")])
        }

        // Look up the account's commitment so we can compare
        // against the in-memory active identity.
        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.server.darkirc.sled.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(vec![notice(nick, format!("No such account: \"{account_name}\""))])
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                // Corrupted account: allow the user to reclaim the
                // tree name. We can't tell whether it's active, so
                // err on the safe side and refuse if there IS an
                // active one. The only way out from a corrupted
                // active account is to manually surgery sled.
                if self.server.rln_identity.read().await.is_some() {
                    return Ok(vec![notice(
                        nick,
                        format!(
                            "Account \"{account_name}\" data is corrupted; refusing to \
                             auto-deregister while another identity is active. SET to \
                             a clean account first, then retry."
                        ),
                    )])
                }
                self.server.darkirc.sled.drop_tree(&tree_name)?;
                return Ok(vec![notice(
                    nick,
                    format!("Dropped corrupted account \"{account_name}\"."),
                )])
            }
        };

        // Refuse if active.
        if let Some(active) = self.server.rln_identity.read().await.as_ref() {
            if active.commitment() == identity.commitment() {
                return Ok(notices(
                    nick,
                    [
                        format!(
                            "\"{account_name}\" is the active identity; refusing to deregister."
                        ),
                        "Use `SET <other_account>` first to switch away.".to_string(),
                    ],
                ))
            }
        }

        // Drop the tree.
        self.server.darkirc.sled.drop_tree(&tree_name)?;

        Ok(vec![notice(nick, format!("Successfully deregistered account \"{account_name}\""))])
    }

    /// Handle the SET command. Swaps the active identity and
    /// persists the choice to the default-mirror tree so it
    /// survives a restart.
    pub async fn handle_set(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        let Some(account_name) = tokens.next() else {
            return Ok(vec![notice(nick, "Invalid syntax. Use `SET <account_name>`.")])
        };
        if account_name == "default" || account_name.is_empty() {
            return Ok(vec![notice(nick, "Invalid account name.")])
        }

        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.server.darkirc.sled.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(notices(
                nick,
                [
                    format!("No such account: \"{account_name}\""),
                    "Use INFO to list registered accounts.".to_string(),
                ],
            ))
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(vec![notice(
                    nick,
                    format!("Account \"{account_name}\" data is corrupted."),
                )])
            }
        };

        // No-op if it's already active. Cheaper than rewriting the
        // default tree, and silences a confusing "now active" line
        // for users who SET twice in a row.
        let already_active = match self.server.rln_identity.read().await.as_ref() {
            Some(active) => active.commitment() == identity.commitment(),
            None => false,
        };
        if already_active {
            return Ok(vec![notice(
                nick,
                format!("\"{account_name}\" is already the active identity."),
            )])
        }

        // Persist the choice. We write the freshly-loaded blob
        // (not the in-memory identity, which would have stale
        // counter state if it were the previously-active one)
        // because the default tree is meant to mirror an account
        // tree exactly.
        let db_default = self.server.darkirc.sled.open_tree(ACCOUNTS_DEFAULT_TREE)?;
        db_default.insert(ACCOUNTS_KEY_RLN_IDENTITY, blob.as_ref())?;

        // Swap in-memory. The new identity comes in with
        // message_id=0 / last_epoch=0; the first send reconciles
        // last_epoch to the current wall-clock epoch and proceeds.
        // The behaviour matches what happens at startup when the
        // default tree is loaded by `IrcServer::new`, so this
        // doesn't introduce a new failure mode.
        *self.server.rln_identity.write().await = Some(identity);

        Ok(notices(
            nick,
            [
                format!("Active identity is now \"{account_name}\"."),
                "If you have used this identity recently from another node, wait one \
                 RLN epoch (10 minutes) before sending to avoid a counter clash."
                    .to_string(),
            ],
        ))
    }

    /// Handle the SLASH command. Publishes a network-wide slash for
    /// the named account, which permanently retires the identity
    /// from the SMT. Unlike DEREGISTER (local only), this affects
    /// the entire network and cannot be undone.
    ///
    /// Refusal rules:
    ///
    /// - The literal `CONFIRM` token is required as the second arg
    ///   to suppress accidents.
    /// - The named account must exist locally (we need its secrets
    ///   to construct the slash proof).
    /// - The account must NOT be the active identity. The user has
    ///   to SET away first - if the goal is to slash the active
    ///   identity the user has to acknowledge they're losing it.
    /// - The local DAG must be synced. The slash proof bakes in
    ///   the current SMT root as a public input; if we built it
    ///   while unsynced, the root might be one no peer recognizes,
    ///   and the slash would be silently rejected. Better to fail
    ///   loudly here than to broadcast a no-op event.
    ///
    /// On success: build proof, broadcast the slash event through
    /// the same canonical pipeline as REGISTER, and drop the local
    /// account tree. Network state and local state both reflect
    /// the retirement after this returns.
    pub async fn handle_slash(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        let Some(account_name) = tokens.next() else {
            return Ok(notices(
                nick,
                [
                    "Invalid syntax. Use `SLASH <account_name> CONFIRM`.",
                    "WARNING: SLASH is permanent and network-wide. See HELP SLASH.",
                ],
            ))
        };
        if account_name == "default" || account_name.is_empty() {
            return Ok(vec![notice(nick, "Invalid account name.")])
        }

        // The CONFIRM token is required to be a literal, not just
        // any non-empty arg, so a fat-fingered "SLASH alice yes"
        // doesn't go through.
        match tokens.next() {
            Some("CONFIRM") => {}
            _ => {
                return Ok(notices(
                    nick,
                    [
                        format!(
                            "SLASH requires explicit confirmation. Type: \
                             SLASH {account_name} CONFIRM"
                        ),
                        "This is permanent and network-wide. See HELP SLASH for the \
                         full warning."
                            .to_string(),
                    ],
                ))
            }
        }

        // Load the account.
        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.server.darkirc.sled.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(vec![notice(nick, format!("No such account: \"{account_name}\""))])
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(vec![notice(
                    nick,
                    format!("Account \"{account_name}\" data is corrupted."),
                )])
            }
        };

        // Refuse if active. Same rule as DEREGISTER, with stronger
        // wording because the consequence is harsher.
        if let Some(active) = self.server.rln_identity.read().await.as_ref() {
            if active.commitment() == identity.commitment() {
                return Ok(notices(
                    nick,
                    [
                        format!("\"{account_name}\" is the active identity; refusing to slash."),
                        "Use `SET <other_account>` first if you genuinely want to slash \
                         this identity."
                            .to_string(),
                    ],
                ))
            }
        }

        // Refuse while unsynced. The slash proof's public input
        // includes the current SMT root, which peers verify against
        // their own historical-roots table. A pre-sync local root
        // is unlikely to match anything peers know about, so the
        // slash would be silently dropped on the receive side.
        let evgr = &self.server.darkirc.event_graph;
        if !evgr.is_synced() {
            return Ok(notices(
                nick,
                [
                    "Cannot SLASH while the local DAG is unsynced.",
                    "Wait for sync to complete and try again.",
                ],
            ))
        }

        // Build the slash proof. `create_slash_proof` takes
        // identity_secret_hash (NOT the raw nullifier+trapdoor pair)
        // because that's what SSS would have recovered in the
        // misbehavior path; the slash circuit's public input is
        // identity_secret_hash + root, and the witness is
        // (identity_secret_hash, merkle_path).
        //
        // The ProvingKey comes from the EG's shared zk_keys cache.
        // The IdentityState write lock is held only for the duration
        // of the proof construction (root read + path computation);
        // the actual proof generation does not need the lock, but
        // the API takes &mut so we hold it for the whole call.
        let identity_secret_hash = identity.identity_secret_hash();
        let slash_pk = evgr.zk_keys.load_slash_pk()?;
        let (proof, root) = {
            let mut id_state = evgr.identity_state.write().await;
            create_slash_proof(identity_secret_hash, &mut id_state, &slash_pk)?
        };

        let slash_blob = SlashBlob { proof, identity_secret_hash, merkle_root: root };
        let blob_bytes = serialize_async(&slash_blob).await;

        let rln_node = RLNNode::Slashing(identity.commitment());
        let event = Event::new_static(serialize_async(&rln_node).await, evgr).await;

        // Apply the slash through the canonical pipeline. The order
        // (apply -> blob_store -> insert -> broadcast) matches the
        // REGISTER path and the receive-side path in
        // proto.rs::handle_static_put.
        evgr.apply_rln_static_event(&event, &rln_node).await?;
        evgr.static_blob_store(&event.id(), &blob_bytes)?;
        evgr.static_insert(&event).await?;
        evgr.static_broadcast(event, blob_bytes).await?;

        // Drop the local account tree. The on-network slash makes
        // the account unusable anyway, so keeping the tree around
        // would be misleading (it would show up in INFO as if it
        // were still registered).
        self.server.darkirc.sled.drop_tree(&tree_name)?;

        Ok(notices(
            nick,
            [
                format!("SLASHED \"{account_name}\". The identity is permanently retired."),
                "The slash event has been broadcast to peers; once propagated, the \
                 commitment is removed from the network's identity tree."
                    .to_string(),
                "Local account state has also been dropped.".to_string(),
            ],
        ))
    }

    /// Reply to the HELP command.
    ///
    /// `HELP` (no args)        -> top-level usage
    /// `HELP <command_name>`   -> per-command help block
    pub async fn handle_help(
        &self,
        nick: &str,
        tokens: &mut SplitAsciiWhitespace<'_>,
    ) -> Result<Vec<ReplyType>> {
        let body = match tokens.next() {
            None => NICKSERV_USAGE,
            Some(sub) => match sub.to_uppercase().as_str() {
                "INFO" => NICKSERV_INFO_HELP,
                "REGISTER" => NICKSERV_REGISTER_HELP,
                "SET" => NICKSERV_SET_HELP,
                "DEREGISTER" => NICKSERV_DEREGISTER_HELP,
                "SLASH" => NICKSERV_SLASH_HELP,
                "HELP" => NICKSERV_USAGE,
                _ => {
                    return Ok(vec![notice(
                        nick,
                        format!("No help available for \"{sub}\". Try `HELP`."),
                    )])
                }
            },
        };

        Ok(notices(nick, body.lines().map(str::to_string)))
    }

    /// Reply to an invalid command
    pub async fn handle_invalid(&self, nick: &str) -> Result<Vec<ReplyType>> {
        Ok(notices(
            nick,
            ["Invalid NickServ command.", "Use /msg NickServ HELP for a NickServ command listing."],
        ))
    }
}

/// Decode a base58-encoded `pallas::Base` scalar. Returns `None`
/// for any malformed input rather than panicking - this is called
/// on user-supplied IRC tokens.
fn parse_pallas_b58(s: &str) -> Option<pallas::Base> {
    let bytes = bs58::decode(s).into_vec().ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    pallas::Base::from_repr(arr).into_option()
}
