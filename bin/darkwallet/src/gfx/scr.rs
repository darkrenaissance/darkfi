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

use async_trait::async_trait;
use darkfi_serial::{SerialEncodable, SerialDecodable, serialize, Encodable, Decodable, deserialize};
use std::{fs::{OpenOptions, File}, time::Instant};

use super::GraphicsMethod;

const FILENAME: &str = "drawinstrs.dat";

#[derive(Debug, SerialEncodable, SerialDecodable)]
struct Instruction {
    timest: u64,
    method: GraphicsMethod
}

pub struct DrawLog {
    instant: Instant,
    fd: File,
}

impl DrawLog {
    pub fn new() -> Self {
        let instant = Instant::now();
        let fd = OpenOptions::new().write(true).create(true).open(FILENAME).unwrap();
        Self {
            instant,
            fd,
        }
    }

    pub fn log(&mut self, method: GraphicsMethod) {
        let instr = Instruction {
            timest: self.instant.elapsed().as_millis() as u64,
            method
        };
        let data = serialize(&instr);
        data.encode(&mut self.fd).unwrap();
    }

    pub fn read() -> Vec<Instruction> {
        let mut instrs = vec![];
        let mut f = File::open(FILENAME).unwrap();
        loop {
            let Ok(data) = Vec::<u8>::decode(&mut f) else { break };

            let instr: Instruction = deserialize(&data).unwrap();
        }
        instrs
    }
}

