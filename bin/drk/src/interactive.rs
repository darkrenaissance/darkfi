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

use linenoise_rs::{
    linenoise, linenoise_history_add, linenoise_history_load, linenoise_history_save,
    linenoise_set_completion_callback, linenoise_set_hints_callback,
};

use darkfi::{cli_desc, system::StoppableTask, util::path::expand_path};

use crate::{
    cli_util::{generate_completions, kaching},
    Drk,
};

// TODO:
//  1. add rest commands handling, along with their completions, hints and help message.
//  2. add input definitions, so you input from files not just stdin.
//  3. add output definitions, so you can output to files not just stdout.
//  4. create a transactions cache in the wallet db, so you can use it to handle them.

/// Auxiliary function to print the help message.
fn help() {
    println!("{}", cli_desc!());
    println!("Commands:");
    println!("\thelp: Prints the help message");
    println!("\tkaching: Fun");
    println!("\tping: Send a ping request to the darkfid RPC endpoint");
    println!("\tcompletions: Generate a SHELL completion script and print to stdout");
}

/// Auxiliary function to define the interactive shell completions.
fn completion(buf: &str, lc: &mut Vec<String>) {
    if buf.starts_with("h") {
        lc.push("help".to_string());
        return
    }

    if buf.starts_with("k") {
        lc.push("kaching".to_string());
        return
    }

    if buf.starts_with("p") {
        lc.push("ping".to_string());
        return
    }

    if buf.starts_with("c") {
        lc.push("completions".to_string());
    }
}

/// Auxiliary function to define the interactive shell hints.
fn hints(buf: &str) -> Option<(String, i32, bool)> {
    match buf {
        "completions " => Some(("{shell}".to_string(), 35, false)), // 35 = magenta
        _ => None,
    }
}

/// Auxiliary function to start provided Drk as an interactive shell.
pub async fn interactive(drk: &Drk, history_path: &str) {
    // Expand the history file path
    let history_path = match expand_path(history_path) {
        Ok(p) => p,
        Err(e) => {
            println!("Error while expanding history file path: {e}");
            return
        }
    };
    let history_path = history_path.into_os_string();
    let history_file = history_path.to_str().unwrap();

    // Set the completion callback. This will be called every time the
    // user uses the <tab> key.
    linenoise_set_completion_callback(completion);

    // Set the shell hints
    linenoise_set_hints_callback(hints);

    // Load history from file.The history file is just a plain text file
    // where entries are separated by newlines.
    let _ = linenoise_history_load(history_file);

    // Create a detached task to use for block subscription
    let subscription_active = false;
    let subscription_task = StoppableTask::new();

    // Start the interactive shell
    loop {
        // Grab input or end if Ctrl-D or Ctrl-C was pressed
        let Some(line) = linenoise("drk> ") else {
            // Stop the subscription task if its active
            if subscription_active {
                subscription_task.stop().await;
            }

            // Write history file
            let _ = linenoise_history_save(history_file);

            return
        };

        // Check if line is empty
        if line.is_empty() {
            continue
        }

        // Add line to history
        linenoise_history_add(&line);

        // Parse command parts
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue
        }

        // Handle command
        match parts[0] {
            "help" => help(),
            "kaching" => kaching().await,
            "ping" => handle_ping(drk).await,
            "completions" => handle_completions(&parts),
            _ => println!("Unreconized command: {}", parts[0]),
        }
    }
}

/// Auxiliary function to define the ping command handling.
async fn handle_ping(drk: &Drk) {
    if let Err(e) = drk.ping().await {
        println!("Error while executing ping command: {e}")
    }
}

/// Auxiliary function to define the completions command handling.
fn handle_completions(parts: &[&str]) {
    if parts.len() != 2 {
        println!("Malformed `completions` command");
        println!("Usage: completions {{shell}}");
        return
    }

    if let Err(e) = generate_completions(parts[1]) {
        println!("Error while executing completions command: {e}")
    }
}
