/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

pub mod error;
pub use error::{ClientFailed, ClientResult, Error, Result};

#[cfg(feature = "blockchain")]
pub mod blockchain;

#[cfg(feature = "validator")]
pub mod validator;

#[cfg(feature = "geode")]
pub mod geode;

#[cfg(feature = "event-graph")]
pub mod event_graph;

#[cfg(feature = "net")]
pub mod net;

#[cfg(feature = "rpc")]
pub mod rpc;

#[cfg(feature = "system")]
pub mod system;

#[cfg(feature = "tx")]
pub mod tx;

#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "wasm-runtime")]
pub mod runtime;

#[cfg(feature = "zk")]
pub mod zk;

#[cfg(feature = "zkas")]
pub mod zkas;

#[cfg(feature = "dht")]
pub mod dht;

pub const ANSI_LOGO: &str = include_str!("../contrib/darkfi.ansi");

#[macro_export]
macro_rules! cli_desc {
    () => {{
        let commitish = match option_env!("COMMITISH") {
            Some(c) => &format!("-{}", c),
            None => "",
        };
        let desc = format!(
            "{} {}\n{}{}\n{}",
            env!("CARGO_PKG_NAME").to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            commitish,
            env!("CARGO_PKG_DESCRIPTION").to_string(),
            darkfi::ANSI_LOGO,
        );

        Box::leak(desc.into_boxed_str()) as &'static str
    }};
}
