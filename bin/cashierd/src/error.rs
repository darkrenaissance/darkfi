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

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Service
    #[error("Services Error: `{0}`")]
    ServicesError(&'static str),
    #[error("Client failed: `{0}`")]
    ClientFailed(String),
    #[cfg(feature = "btc")]
    #[error(transparent)]
    BtcFailed(#[from] crate::service::BtcFailed),
    #[cfg(feature = "sol")]
    #[error("Sol client failed: `{0}`")]
    SolFailed(String),
    #[cfg(feature = "eth")]
    #[error(transparent)]
    EthFailed(#[from] crate::service::EthFailed),
    #[error("BridgeError Error: `{0}`")]
    BridgeError(String),

    #[error("Async_channel sender error")]
    AsyncChannelSenderError,
    #[error(transparent)]
    AsyncChannelReceiverError(#[from] async_channel::RecvError),
}

#[cfg(feature = "sol")]
impl From<crate::service::SolFailed> for Error {
    fn from(err: crate::service::SolFailed) -> Error {
        Error::SolFailed(err.to_string())
    }
}

impl From<darkfi::Error> for Error {
    fn from(err: darkfi::Error) -> Error {
        Error::ClientFailed(err.to_string())
    }
}

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_err: async_channel::SendError<T>) -> Error {
        Error::AsyncChannelSenderError
    }
}
