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

#![no_main]
use libfuzzer_sys::fuzz_target;

use darkfi::zkas::Lexer;

// Run with: ZKAS_SILENT=1 cargo fuzz run zkas-lexer

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let filename = "fuzz0r";
        let source = source.replace('\t', "    ").replace("\r\n", "\n");
        let lexer = Lexer::new(filename, source.chars());
        let _ = lexer.lex();
    }
});
