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
use std::{io::Cursor, process::exit};

use darkfi::{
    util::{async_util::sleep, parse::decode_base10},
    Result,
};
use darkfi_sdk::crypto::TokenId;
use rodio::{source::Source, Decoder, OutputStream};

use super::Drk;

pub fn parse_value_pair(s: &str) -> Result<(u64, u64)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        eprintln!("Invalid value pair. Use a pair such as 13.37:11.0");
        exit(1);
    }

    // TODO: We shouldn't be hardcoding everything to 8 decimals.
    let val0 = decode_base10(v[0], 8, true);
    let val1 = decode_base10(v[1], 8, true);

    if val0.is_err() || val1.is_err() {
        eprintln!("Invalid value pair. Use a pair such as 13.37:11.0");
        exit(1);
    }

    Ok((val0.unwrap(), val1.unwrap()))
}

pub async fn parse_token_pair(drk: &Drk, s: &str) -> Result<(TokenId, TokenId)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        eprintln!("Invalid token pair. Use a pair such as:");
        eprintln!("WCKD:MLDY");
        eprintln!("or");
        eprintln!("A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2");
        exit(1);
    }

    let tok0 = drk.get_token(v[0].to_string()).await;
    let tok1 = drk.get_token(v[1].to_string()).await;

    if tok0.is_err() || tok1.is_err() {
        eprintln!("Invalid token pair. Use a pair such as:");
        eprintln!("WCKD:MLDY");
        eprintln!("or");
        eprintln!("A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2");
        exit(1);
    }

    Ok((tok0.unwrap(), tok1.unwrap()))
}

/// Fun police go away
pub async fn kaching() {
    const WALLET_MP3: &[u8] = include_bytes!("../wallet.mp3");

    let cursor = Cursor::new(WALLET_MP3);

    let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
        return
    };

    let Ok(source) = Decoder::new(cursor) else {
        return
    };

    if stream_handle.play_raw(source.convert_samples()).is_err() {
        return
    }

    sleep(2).await;
}
