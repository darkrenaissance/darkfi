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

/// Initializes logging for test cases, which is useful for debugging issues encountered during testing.
/// The logger is configured based on the provided list of targets to ignore and the desired log level.
#[cfg(test)]
pub fn init_logger(log_level: simplelog::LevelFilter, ignore_targets: Vec<&str>) {
    let mut cfg = simplelog::ConfigBuilder::new();

    // Add targets to ignore
    for target in ignore_targets {
        cfg.add_filter_ignore(target.to_string());
    }

    // Set log level
    cfg.set_target_level(log_level);

    // initialize the logger
    if simplelog::TermLogger::init(
        log_level,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .is_err()
    {
        // Print an error message if logger failed to initialize
        eprintln!("Logger failed to initialize");
    }
}
