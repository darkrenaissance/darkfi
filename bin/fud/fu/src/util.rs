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

use termcolor::{Color, ColorSpec};

pub fn status_to_colorspec(status: &str) -> ColorSpec {
    ColorSpec::new()
        .set_fg(match status {
            "downloading" => Some(Color::Blue),
            "seeding" => Some(Color::Green),
            "discovering" => Some(Color::Magenta),
            "incomplete" => Some(Color::Red),
            "verifying" => Some(Color::Yellow),
            _ => None,
        })
        .set_bold(true)
        .clone()
}

pub fn type_to_colorspec(rtype: &str) -> ColorSpec {
    ColorSpec::new()
        .set_fg(match rtype {
            "file" => Some(Color::Blue),
            "directory" => Some(Color::Magenta),
            _ => None,
        })
        .set_bold(true)
        .clone()
}
