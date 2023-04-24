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

use std::collections::HashMap;

use darkfi::Result;

use crate::{
    settings::{Args, ChannelInfo, ContactInfo},
    PrivMsgEvent,
};

mod client;
pub use client::IrcClient;

mod server;
pub use server::IrcServer;

#[derive(Clone)]
pub struct IrcConfig {
    // init bool
    pub is_nick_init: bool,
    pub is_user_init: bool,
    pub is_registered: bool,
    pub is_cap_end: bool,
    pub is_pass_init: bool,

    // user config
    pub nickname: String,
    pub password: String,
    pub private_key: Option<String>,
    pub capabilities: HashMap<String, bool>,

    // channels and contacts
    pub channels: HashMap<String, ChannelInfo>,
    pub contacts: HashMap<String, ContactInfo>,
}

impl IrcConfig {
    pub fn new(settings: &Args) -> Result<Self> {
        let password = settings.password.as_ref().unwrap_or(&String::new()).clone();
        let private_key = settings.private_key.clone();

        let mut channels = settings.channels.clone();

        for chan in settings.autojoin.iter() {
            if !channels.contains_key(chan) {
                channels.insert(chan.clone(), ChannelInfo::new());
            }
        }

        let contacts = settings.contacts.clone();

        let mut capabilities = HashMap::new();
        capabilities.insert("no-history".to_string(), false);

        Ok(Self {
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            is_cap_end: true,
            is_pass_init: false,
            nickname: "anon".to_string(),
            password,
            channels,
            contacts,
            private_key,
            capabilities,
        })
    }
}

#[derive(Clone)]
pub enum ClientSubMsg {
    Privmsg(PrivMsgEvent),
    Config(IrcConfig),
}
#[derive(Clone)]
pub enum NotifierMsg {
    Privmsg(PrivMsgEvent),
    UpdateConfig,
}
