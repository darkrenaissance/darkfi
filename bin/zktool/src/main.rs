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

use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum CliSubCommands {
    /// Create a random Base value
    Random {},
    /// Convert int to Base
    FromInt {
        value: u64,
    },
    Add {
        value_a: String,
        value_b: String,
    },
    Sub {
        value_a: String,
        value_b: String,
    },
    Mul {
        value_a: String,
        value_b: String,
    },
    MakeProof {
        bincode: String,
        witness: String,
        publics: String,
    },
    VerifyProof {
        bincode: String,
        publics: String,
    },
}

#[derive(Parser)]
#[clap(name = "zktool")]
#[clap(arg_required_else_help(true))]
pub struct CliDao {
    /// Increase verbosity
    #[clap(short, action = clap::ArgAction::Count)]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliSubCommands>,
}

fn main() {
    let args = CliDao::parse();
    match args.command {
        Some(_) => {
            println!("Some arg!");
        }

        None => todo!(),
    }
}
