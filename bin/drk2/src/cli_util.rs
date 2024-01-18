/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
use std::io::Cursor;

use rodio::{source::Source, Decoder, OutputStream};

use darkfi::system::sleep;

/// Fun police go away
pub async fn kaching() {
    const WALLET_MP3: &[u8] = include_bytes!("../wallet.mp3");

    let cursor = Cursor::new(WALLET_MP3);

    let Ok((_stream, stream_handle)) = OutputStream::try_default() else { return };

    let Ok(source) = Decoder::new(cursor) else { return };

    if stream_handle.play_raw(source.convert_samples()).is_err() {
        return
    }

    sleep(2).await;
}
