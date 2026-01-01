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

use std::collections::HashMap;

use darkfi::{util::parse::encode_base10, zk::halo2::Field};
use darkfi_money_contract::{client::OwnCoin, model::TokenId};
use darkfi_sdk::{
    crypto::{
        keypair::{Address, Network, PublicKey, SecretKey, StandardAddress},
        BaseBlind, ContractId, FuncId,
    },
    pasta::pallas,
};
use darkfi_serial::serialize;
use prettytable::{format, row, Table};

use crate::money::BALANCE_BASE10_DECIMALS;

pub fn prettytable_addrs(
    network: Network,
    addresses: &[(u64, PublicKey, SecretKey, u64)],
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Key ID", "Address", "Public Key", "Secret Key", "Is Default"]);
    for (key_id, public_key, secret_key, is_default) in addresses {
        let is_default = match is_default {
            1 => "*",
            _ => "",
        };

        let address: Address = StandardAddress::from_public(network, *public_key).into();
        table.add_row(row![key_id, address, public_key, secret_key, is_default]);
    }

    table
}

pub fn prettytable_balance(
    balmap: &HashMap<String, u64>,
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Token ID", "Aliases", "Balance"]);

    for (token_id, balance) in balmap.iter() {
        let alias = match alimap.get(token_id) {
            Some(v) => v,
            None => "-",
        };

        table.add_row(row![token_id, alias, encode_base10(*balance, BALANCE_BASE10_DECIMALS)]);
    }

    table
}

pub fn prettytable_coins(
    coins: &[(OwnCoin, u32, bool, Option<u32>, String)],
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![
        "Coin",
        "Token ID",
        "Aliases",
        "Value",
        "Spend Hook",
        "User Data",
        "Creation Height",
        "Spent",
        "Spent Height",
        "Spent TX",
    ]);

    for coin in coins {
        let alias = match alimap.get(&coin.0.note.token_id.to_string()) {
            Some(v) => v,
            None => "-",
        };

        let spend_hook = if coin.0.note.spend_hook != FuncId::none() {
            format!("{}", coin.0.note.spend_hook)
        } else {
            String::from("-")
        };

        let user_data = if coin.0.note.user_data != pallas::Base::ZERO {
            bs58::encode(serialize(&coin.0.note.user_data)).into_string().to_string()
        } else {
            String::from("-")
        };

        let spent_height = match coin.3 {
            Some(spent_height) => spent_height.to_string(),
            None => String::from("-"),
        };

        table.add_row(row![
            bs58::encode(&serialize(&coin.0.coin.inner())).into_string().to_string(),
            coin.0.note.token_id,
            alias,
            format!(
                "{} ({})",
                coin.0.note.value,
                encode_base10(coin.0.note.value, BALANCE_BASE10_DECIMALS)
            ),
            spend_hook,
            user_data,
            coin.1,
            coin.2,
            spent_height,
            coin.4,
        ]);
    }

    table
}

pub fn prettytable_tokenlist(
    tokens: &[(TokenId, SecretKey, BaseBlind, bool, Option<u32>)],
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![
        "Token ID",
        "Aliases",
        "Mint Authority",
        "Token Blind",
        "Frozen",
        "Freeze Height",
    ]);

    for (token_id, authority, blind, frozen, freeze_height) in tokens {
        let alias = match alimap.get(&token_id.to_string()) {
            Some(v) => v,
            None => "-",
        };

        let freeze_height = match freeze_height {
            Some(freeze_height) => freeze_height.to_string(),
            None => String::from("-"),
        };

        table.add_row(row![token_id, alias, authority, blind, frozen, freeze_height]);
    }

    table
}

pub fn prettytable_contract_history(deploy_history: &[(String, String, u32)]) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Transaction Hash", "Type", "Block Height"]);

    for (tx_hash, tx_type, block_height) in deploy_history {
        table.add_row(row![tx_hash, tx_type, block_height]);
    }

    table
}

pub fn prettytable_contract_auth(auths: &[(ContractId, SecretKey, bool, Option<u32>)]) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Contract ID", "Secret Key", "Locked", "Lock Height"]);

    for (contract_id, secret_key, is_locked, lock_height) in auths {
        let lock_height = match lock_height {
            Some(lock_height) => lock_height.to_string(),
            None => String::from("-"),
        };

        table.add_row(row![contract_id, secret_key, is_locked, lock_height]);
    }

    table
}

pub fn prettytable_aliases(alimap: &HashMap<String, TokenId>) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Alias", "Token ID"]);

    for (alias, token_id) in alimap.iter() {
        table.add_row(row![alias, token_id]);
    }

    table
}

pub fn prettytable_scanned_blocks(scanned_blocks: &[(u32, String, String)]) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Height", "Hash", "Signing Key"]);
    for (height, hash, signing_key) in scanned_blocks {
        table.add_row(row![height, hash, signing_key]);
    }

    table
}
