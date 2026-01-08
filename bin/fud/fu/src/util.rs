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

use std::io::Write;

use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use fud::resource::{ResourceStatus, ResourceType};

const UNITS: [&str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

pub fn status_to_colorspec(status: &ResourceStatus) -> ColorSpec {
    ColorSpec::new()
        .set_fg(match status {
            ResourceStatus::Downloading => Some(Color::Blue),
            ResourceStatus::Seeding => Some(Color::Green),
            ResourceStatus::Discovering => Some(Color::Magenta),
            ResourceStatus::Incomplete(_) => Some(Color::Red),
            ResourceStatus::Verifying => Some(Color::Yellow),
        })
        .set_bold(true)
        .clone()
}

pub fn type_to_colorspec(rtype: &ResourceType) -> ColorSpec {
    ColorSpec::new()
        .set_fg(match rtype {
            ResourceType::File => Some(Color::Blue),
            ResourceType::Directory => Some(Color::Magenta),
            ResourceType::Unknown => None,
        })
        .set_bold(true)
        .clone()
}

pub fn format_bytes(bytes: u64) -> String {
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{size:.1} {}", UNITS[unit_index])
}

pub fn format_progress_bytes(current: u64, total: u64) -> String {
    let mut total = total as f64;
    let mut unit_index = 0;

    while total >= 1024.0 && unit_index < UNITS.len() - 1 {
        total /= 1024.0;
        unit_index += 1;
    }

    let current = (current as f64) / 1024_f64.powi(unit_index as i32);

    format!("{current:.1}/{total:.1} {}", UNITS[unit_index])
}

/// Returns a formated string from the duration.
/// - 1 -> 1s
/// - 60 -> 1m
/// - 90 -> 1m30s
pub fn format_duration(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }

    let units = [
        (86400, "d"), // days
        (3600, "h"),  // hours
        (60, "m"),    // minutes
        (1, "s"),     // seconds
    ];

    for (i, (unit_seconds, unit_symbol)) in units.iter().enumerate() {
        if seconds >= *unit_seconds {
            let first = seconds / unit_seconds;
            let remaining = seconds % unit_seconds;

            if remaining > 0 && i < units.len() - 1 {
                let (next_unit_seconds, next_unit_symbol) = units[i + 1];
                let second = remaining / next_unit_seconds;
                return format!("{first}{unit_symbol}{second}{next_unit_symbol}");
            }

            return format!("{first}{unit_symbol}");
        }
    }

    "0s".to_string()
}

/// Tree only used for printing.
#[derive(Debug)]
pub struct TreeNode<K> {
    pub key: K,
    pub value: Option<String>,
    pub color: Option<ColorSpec>,
    pub children: Vec<TreeNode<K>>,
}
impl<K> TreeNode<K> {
    /// Key only
    pub fn key(key: K) -> Self {
        Self { key, value: None, color: None, children: vec![] }
    }
    /// Key + value
    pub fn kv(key: K, value: String) -> Self {
        Self { key, value: Some(value), color: None, children: vec![] }
    }
    /// Key + value + color
    pub fn kvc(key: K, value: String, color: ColorSpec) -> Self {
        Self { key, value: Some(value), color: Some(color), children: vec![] }
    }
}

pub fn print_tree<K: AsRef<str> + std::fmt::Display>(root: &str, items: &[TreeNode<K>]) {
    fn print_node<K: AsRef<str> + std::fmt::Display>(
        node: &TreeNode<K>,
        is_last: bool,
        prefix: &str,
    ) {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);

        write!(&mut stdout, "{}{} {}", prefix, if is_last { "└─" } else { "├─" }, node.key)
            .unwrap();

        if let Some(value) = &node.value {
            write!(&mut stdout, ": ").unwrap();
            if let Some(spec) = &node.color {
                stdout.set_color(spec).unwrap();
            }
            write!(&mut stdout, "{value}").unwrap();
            stdout.reset().unwrap();
        }

        writeln!(&mut stdout).unwrap();

        let new_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });

        for (i, child) in node.children.iter().enumerate() {
            print_node(child, i == node.children.len() - 1, &new_prefix);
        }
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    stdout.set_color(ColorSpec::new().set_bold(true)).unwrap();
    writeln!(&mut stdout, "{root}").unwrap();
    stdout.reset().unwrap();

    for (i, item) in items.iter().enumerate() {
        print_node(item, i == items.len() - 1, "");
    }
}

macro_rules! optional_value {
    ($value:expr) => {
        match $value {
            0 => "?".to_string(),
            x => x.to_string(),
        }
    };
    ($value:expr, $formatter:expr) => {
        match $value {
            0 => "?".to_string(),
            x => $formatter(x),
        }
    };
}
pub(crate) use optional_value;
