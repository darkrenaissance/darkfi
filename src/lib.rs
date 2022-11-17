/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

//#![feature(let_else)]

pub mod error;
pub use error::{ClientFailed, ClientResult, Error, Result, VerifyFailed, VerifyResult};

#[cfg(feature = "blockchain")]
pub mod blockchain;

#[cfg(feature = "blockchain")]
pub mod consensus;

#[cfg(feature = "crypto")]
pub mod crypto;

#[cfg(feature = "crypto")]
pub mod zk;

#[cfg(feature = "dht")]
pub mod dht;

#[cfg(feature = "net")]
pub mod net;

//#[cfg(feature = "node")]
//pub mod node;

#[cfg(feature = "raft")]
pub mod raft;

#[cfg(feature = "rpc")]
pub mod rpc;

#[cfg(feature = "system")]
pub mod system;

#[cfg(feature = "tx")]
pub mod tx;

#[cfg(feature = "tx")]
pub mod tx2;

#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "wallet")]
pub mod wallet;

#[cfg(feature = "wasm-runtime")]
pub mod runtime;

#[cfg(feature = "zkas")]
pub mod zkas;

pub const ANSI_LOGO: &str = include_str!("../contrib/darkfi.ansi");

#[macro_export]
macro_rules! cli_desc {
    () => {{
        let desc = format!(
            "{} {}\n{}\n{}",
            env!("CARGO_PKG_NAME").to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            env!("CARGO_PKG_DESCRIPTION").to_string(),
            darkfi::ANSI_LOGO,
        );

        Box::leak(desc.into_boxed_str()) as &'static str
    }};
}
