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

use darkfi::tx::Transaction;
use darkfi_money_contract::model::TokenId;
use darkfi_sdk::crypto::keypair::Address;

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 50.;
    pub const BACKARROW_Y: f32 = 70.;
    pub const TITLE_FONTSIZE: f32 = 56.;
    pub const BUTTON_FONTSIZE: f32 = 48.;
    pub const BASE_FONTSIZE: f32 = 48.;
    pub const HINT_FONTSIZE: f32 = BASE_FONTSIZE * 0.8;
    pub const AMOUNT_FONTSIZE: f32 = 118.;
    pub const AMOUNT_CHAR_WIDTH: f32 = AMOUNT_FONTSIZE * 0.6;
    pub const AMOUNT_TOKEN_SPACING: f32 = AMOUNT_FONTSIZE * 0.35;
    pub const BUTTON_HEIGHT: f32 = 200.;
    pub const TOKEN_ROW_HEIGHT: f32 = 80.;
    pub const TITLE_PADDING: f32 = 50.;
    pub const TOKEN_NAME_OFFSET: f32 = 200.;
    pub const PADDING_X: f32 = 40.;
    pub const PADDING_Y: f32 = 30.;
    pub const RECIPIENT_INPUT_MARGIN: f32 = 30.;
    pub const RECIPIENT_INPUT_PADDING_X: f32 = 40.;
    pub const RECIPIENT_INPUT_HEIGHT: f32 = 120.;
    pub const RECIPIENT_INPUT_FONTSIZE: f32 = 48.;
    pub const HEADER_HEIGHT: f32 = 140.;
    pub const ROW_HEIGHT: f32 = 80.;
    pub const WALLET_BTN_SIZE: f32 = 50.;
    pub const COPY_WIDTH: f32 = 200.;
    pub const COPY_SCALE: f32 = 30.;

    pub const NETSTATUS_ICON_SIZE: f32 = 140.;
    pub const SETTINGS_ICON_SIZE: f32 = 140.;
    pub const NETLOGO_SCALE: f32 = 7.;
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 120.;
}

#[cfg(target_os = "android")]
mod ui_consts {
    pub use super::android_ui_consts::*;
}

#[cfg(feature = "emulate-android")]
mod ui_consts {
    pub use super::android_ui_consts::*;
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 26.;
    pub const TITLE_FONTSIZE: f32 = 20.;
    pub const BUTTON_FONTSIZE: f32 = 20.;
    pub const BASE_FONTSIZE: f32 = 20.;
    pub const HINT_FONTSIZE: f32 = BASE_FONTSIZE * 0.8;
    pub const AMOUNT_FONTSIZE: f32 = 56.;
    pub const AMOUNT_CHAR_WIDTH: f32 = AMOUNT_FONTSIZE * 0.6;
    pub const AMOUNT_TOKEN_SPACING: f32 = AMOUNT_FONTSIZE * 0.35;
    pub const BUTTON_HEIGHT: f32 = 90.;
    pub const TOKEN_ROW_HEIGHT: f32 = 40.;
    pub const TITLE_PADDING: f32 = 25.;
    pub const TOKEN_NAME_OFFSET: f32 = 90.;
    pub const PADDING_X: f32 = 20.;
    pub const PADDING_Y: f32 = 15.;
    pub const RECIPIENT_INPUT_MARGIN: f32 = 15.;
    pub const RECIPIENT_INPUT_PADDING_X: f32 = 20.;
    pub const RECIPIENT_INPUT_HEIGHT: f32 = 60.;
    pub const RECIPIENT_INPUT_FONTSIZE: f32 = 20.;
    pub const HEADER_HEIGHT: f32 = 60.;
    pub const ROW_HEIGHT: f32 = 80.;
    pub const WALLET_BTN_SIZE: f32 = 50.;
    pub const COPY_WIDTH: f32 = 100.;
    pub const COPY_SCALE: f32 = 15.;

    pub const NETSTATUS_ICON_SIZE: f32 = 60.;
    pub const SETTINGS_ICON_SIZE: f32 = 60.;
    pub const NETLOGO_SCALE: f32 = 3.5;
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 50.;
}

pub use ui_consts::*;

pub const BALANCE_BASE10_DECIMALS: usize = 8;

// Send transaction data shared across all send pages
#[derive(Debug, Clone)]
pub struct SendTxData {
    pub token_symbol: Option<String>,
    pub token_name: Option<String>,
    pub token_id: Option<TokenId>,
    pub recipient_str: Option<String>,
    pub recipient: Option<Address>,
    pub amount: Option<String>,
    pub tx_built: bool,
    pub tx: Option<Transaction>,
}

impl SendTxData {
    pub fn new() -> Self {
        Self {
            token_symbol: None,
            token_name: None,
            token_id: None,
            recipient_str: None,
            recipient: None,
            amount: None,
            tx_built: false,
            tx: None,
        }
    }
}
