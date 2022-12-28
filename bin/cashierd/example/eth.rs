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

use num_bigint::BigUint;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    service::eth::{erc20_transfer_data, EthClient, EthTx},
    util::{decode_base10, encode_base10},
    Result,
};

#[async_std::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Trace,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let acc = "0x113b6648f34f4d0340d04ff171cbcf0b49d47827".to_string();
    let key = "67cbb73cb293eea5fa2a7025d5479dbd50319010c03fd8821917ad0d9d53276c".to_string();

    let mut eth = EthClient::new("", "/home/parazyd/.ethereum/ropsten/geth.ipc", "foobar");

    eth.main_keypair.private_key = key;
    eth.main_keypair.public_key = acc.clone();

    //let key = generate_privkey();
    //let passphrase = "foobar".to_string();
    //let rep = eth.import_privkey(&key, &passphrase).await?;
    //println!("{:#?}", rep);

    let passphrase = "foobar".to_string();

    // Recipient address
    let dest = "0xcD640A363305c21255c58Ba9C8c1C508e6997a12".to_string();

    // Latest known block, used to calculate present balance.
    let block = eth.block_number().await?;
    let block = block.as_str().unwrap();

    // Native ETH balance
    let hexbalance = eth.get_eth_balance(&acc, block).await?;
    let hexbalance = hexbalance.as_str().unwrap().trim_start_matches("0x");
    let balance = BigUint::parse_bytes(hexbalance.as_bytes(), 16).unwrap();
    println!("{}", encode_base10(balance, 18));

    /*
    // Transfer native ETH
    let tx = EthTx::new(
    &acc,
    &dest,
    None,
    None,
    Some(decode_base10("0.051", 18, true)?),
    None,
    None,
    );

    let rep = eth.send_transaction(&tx, &passphrase).await?;
    println!("TXID: {}", rep.as_str().unwrap());
    */

    // ERC20 Token balance
    let mint = "0xad6d458402f60fd3bd25163575031acdce07538d"; // Ropsten DAI (get on Uniswap)
    let hexbalance = eth.get_erc20_balance(&acc, mint).await?;
    let hexbalance = hexbalance.as_str().unwrap().trim_start_matches("0x");
    let balance = BigUint::parse_bytes(hexbalance.as_bytes(), 16).unwrap();
    println!("{}", encode_base10(balance, 18));

    // Transfer ERC20 token
    let tx = EthTx::new(
        &acc,
        mint,
        None,
        None,
        None,
        Some(erc20_transfer_data(&dest, decode_base10("1", 18, true)?)),
        None,
    );

    let rep = eth.send_transaction(&tx, &passphrase).await?;
    println!("TXID: {}", rep.as_str().unwrap());

    Ok(())
}
