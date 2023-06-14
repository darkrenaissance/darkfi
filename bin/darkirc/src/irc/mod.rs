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

use darkfi::{util::path::get_config_path, Result};

use crate::{
    settings::{
        parse_configured_channels, parse_configured_contacts, Args, ChannelInfo, ContactInfo,
        CONFIG_FILE,
    },
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
    pub capabilities: HashMap<String, bool>,

    // channels and contacts
    pub auto_channels: Vec<String>,
    pub channels: HashMap<String, ChannelInfo>,
    pub contacts: HashMap<String, ContactInfo>,
}

impl IrcConfig {
    pub fn new(settings: &Args) -> Result<Self> {
        let password = settings.password.as_ref().unwrap_or(&String::new()).clone();

        let auto_channels = settings.autojoin.clone();

        // Pick up channel settings from the TOML configuration
        let cfg_path = get_config_path(settings.config.clone(), CONFIG_FILE)?;
        let toml_contents = std::fs::read_to_string(cfg_path)?;
        let channels = parse_configured_channels(&toml_contents)?;
        let contacts = parse_configured_contacts(&toml_contents)?;

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
            auto_channels,
            channels,
            contacts,
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
