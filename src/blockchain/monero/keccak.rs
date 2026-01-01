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

use std::io::{Cursor, Read, Result, Write};

#[allow(unused_imports)]
use tiny_keccak::{Hasher, Keccak};

#[repr(C)]
#[allow(unused)]
enum Mode {
    Absorbing,
    Squeezing,
}

#[repr(C)]
// https://docs.rs/tiny-keccak/latest/src/tiny_keccak/lib.rs.html#368
struct KeccakState {
    buffer: [u8; 200],
    offset: usize,
    rate: usize,
    delim: u8,
    mode: Mode,
}

unsafe fn serialize_keccak<W: Write>(keccak: &Keccak, writer: &mut W) -> Result<()> {
    let keccak_ptr = keccak as *const Keccak as *const KeccakState;
    let keccak_state = &*keccak_ptr;

    writer.write_all(&keccak_state.buffer)?;
    writer.write_all(&(keccak_state.offset as u64).to_le_bytes())?;
    writer.write_all(&(keccak_state.rate as u64).to_le_bytes())?;
    writer.write_all(&[keccak_state.delim])?;

    Ok(())
}

unsafe fn deserialize_keccak<R: Read>(reader: &mut R) -> Result<Keccak> {
    let mut keccak = Keccak::v256();

    let keccak_ptr = &mut keccak as *mut Keccak as *mut KeccakState;
    let keccak_state = &mut *keccak_ptr;

    reader.read_exact(&mut keccak_state.buffer)?;

    let mut offset_bytes = [0u8; 8];
    reader.read_exact(&mut offset_bytes)?;
    keccak_state.offset = u64::from_le_bytes(offset_bytes) as usize;

    let mut rate_bytes = [0u8; 8];
    reader.read_exact(&mut rate_bytes)?;
    keccak_state.rate = u64::from_le_bytes(rate_bytes) as usize;

    let mut delim_byte = [0u8; 1];
    reader.read_exact(&mut delim_byte)?;
    keccak_state.delim = delim_byte[0];

    keccak_state.mode = Mode::Absorbing;

    Ok(keccak)
}

pub fn keccak_to_bytes(keccak: &Keccak) -> Vec<u8> {
    let mut bytes = vec![];
    unsafe { serialize_keccak(keccak, &mut bytes).unwrap() }
    bytes
}

pub fn keccak_from_bytes(bytes: &[u8]) -> Keccak {
    let mut cursor = Cursor::new(bytes);
    unsafe { deserialize_keccak(&mut cursor).unwrap() }
}

#[test]
fn test_keccak_serde() {
    let mut keccak = Keccak::v256();
    keccak.update(b"foobar");

    let ser = keccak_to_bytes(&keccak);

    let mut digest1 = [0u8; 32];
    keccak.finalize(&mut digest1);

    let de = keccak_from_bytes(&ser);
    let mut digest2 = [0u8; 32];
    de.finalize(&mut digest2);

    println!("{digest1:?}");
    println!("{digest2:?}");

    assert_eq!(digest1, digest2);
}
