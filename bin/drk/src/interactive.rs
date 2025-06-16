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

use std::{io::ErrorKind, str::FromStr};

use futures::{select, FutureExt};
use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};
use linenoise_rs::{
    linenoise_history_add, linenoise_history_load, linenoise_history_save,
    linenoise_set_completion_callback, linenoise_set_hints_callback, LinenoiseState,
};
use smol::channel::{unbounded, Receiver, Sender};

use darkfi::{
    cli_desc,
    system::{msleep, ExecutorPtr, StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error,
};

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
    println!(
        "\tsubscribe: Perform a scan and then subscribe to darkfid to listen for incoming blocks"
    );
    println!("\tunsubscribe: Stops the background subscription, if its active");
    println!("\tsnooze: Disables the background subscription messages printing");
    println!("\tunsnooze: Enables the background subscription messages printing");
    println!("\tscan: Scan the blockchain and parse relevant transactions");
}

/// Auxiliary function to define the interactive shell completions.
fn completion(buf: &str, lc: &mut Vec<String>) {
    // First we define the specific commands prefixes
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
        return
    }

    if buf.starts_with("su") {
        lc.push("subscribe".to_string());
        return
    }

    if buf.starts_with("unsu") {
        lc.push("unsubscribe".to_string());
        return
    }

    if buf.starts_with("sn") {
        lc.push("snooze".to_string());
        return
    }

    if buf.starts_with("unsn") {
        lc.push("unsnooze".to_string());
        return
    }

    if buf.starts_with("sc") {
        lc.push("scan".to_string());
        return
    }

    // Now the catch alls
    if buf.starts_with("s") {
        lc.push("subscribe".to_string());
        lc.push("snooze".to_string());
        lc.push("scan".to_string());
        return
    }

    if buf.starts_with("u") {
        lc.push("unsubscribe".to_string());
        lc.push("unsnooze".to_string());
    }
}

/// Auxiliary function to define the interactive shell hints.
fn hints(buf: &str) -> Option<(String, i32, bool)> {
    match buf {
        "completions " => Some(("{shell}".to_string(), 35, false)), // 35 = magenta
        "scan " => Some(("--reset {height}".to_string(), 35, false)), // 35 = magenta
        _ => None,
    }
}

/// Auxiliary function to start provided Drk as an interactive shell.
/// Only sane/linenoise terminals are suported.
pub async fn interactive(drk: &Drk, history_path: &str, ex: &ExecutorPtr) {
    // Expand the history file path
    let history_path = match expand_path(history_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error while expanding history file path: {e}");
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
    let mut subscription_active = false;
    let mut snooze_active = false;
    let subscription_task = StoppableTask::new();

    // Create an unbounded smol channel, so we can have a printing
    // queue the background task can submit messages to the shell.
    let (shell_sender, shell_receiver) = unbounded();

    // Start the interactive shell
    loop {
        // Wait for next line to process
        let line = listen_for_line(&snooze_active, &shell_receiver).await;

        // Grab input or end if Ctrl-D or Ctrl-C was pressed
        let Some(line) = line else { break };

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
            "subscribe" => {
                handle_subscribe(
                    drk,
                    &mut subscription_active,
                    &subscription_task,
                    &shell_sender,
                    ex,
                )
                .await
            }
            "unsubscribe" => handle_unsubscribe(&mut subscription_active, &subscription_task).await,
            "snooze" => snooze_active = true,
            "unsnooze" => snooze_active = false,
            "scan" => handle_scan(drk, &subscription_active, &parts).await,
            _ => println!("Unreconized command: {}", parts[0]),
        }
    }

    // Stop the subscription task if its active
    if subscription_active {
        subscription_task.stop().await;
    }

    // Write history file
    let _ = linenoise_history_save(history_file);
}

/// Auxiliary function to listen for linenoise input line and handle
/// background task messages.
async fn listen_for_line(
    snooze_active: &bool,
    shell_receiver: &Receiver<Vec<String>>,
) -> Option<String> {
    // Generate the linoise state structure
    let mut state = match LinenoiseState::edit_start(-1, -1, "drk> ") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error while generating linenoise state: {e}");
            return None
        }
    };

    // Set stdin to non-blocking mode
    let fd = state.get_fd();
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        fcntl(fd, F_SETFL, flags | O_NONBLOCK);
    }

    // Read until we get a line to process
    let mut line = None;
    loop {
        // Future that polls stdin for input
        let input_future = async {
            loop {
                match state.edit_feed() {
                    Ok(Some(l)) => {
                        line = Some(l);
                        break
                    }
                    Ok(None) => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => break,
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        // No data available, yield and retry
                        msleep(10).await;
                        continue
                    }
                    Err(e) => {
                        eprintln!("Error while reading linenoise feed: {e}");
                        break
                    }
                }
            }
        };

        // Future that polls the channel
        let channel_future = async {
            loop {
                if !shell_receiver.is_empty() {
                    break
                }
                msleep(1000).await;
            }
        };

        // Manage the futures
        select! {
            // When input is ready we break out the loop
            _ = input_future.fuse() => break,
            // Manage filled channel
            _ = channel_future.fuse() => {
                while !shell_receiver.is_empty() {
                    match shell_receiver.recv().await {
                        Ok(msg) => {
                            // We only print if snooze is inactive,
                            // but have to consume the message regardless,
                            // so the queue gets empty.
                            if *snooze_active {
                                continue
                            }
                            // Hide prompt, print output, show prompt again
                            let _ = state.hide();
                            for line in msg {
                                println!("{line}\r");
                            }
                            let _ = state.show();
                        }
                        Err(e) => {
                            eprintln!("Error while reading shell receiver channel: {e}");
                            break
                        }
                    }
                }
            }
        }
    }

    // Restore blocking mode
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        fcntl(fd, F_SETFL, flags & !O_NONBLOCK);
    }

    let _ = state.edit_stop();
    line
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

/// Auxiliary function to define the subscribe command handling.
async fn handle_subscribe(
    drk: &Drk,
    subscription_active: &mut bool,
    subscription_task: &StoppableTaskPtr,
    shell_sender: &Sender<Vec<String>>,
    ex: &ExecutorPtr,
) {
    if *subscription_active {
        println!("Subscription is already active!")
    }

    if let Err(e) = drk.scan_blocks().await {
        println!("Failed during scanning: {e:?}");
        return
    }
    println!("Finished scanning blockchain");

    // Start the subcristion task
    // TODO: use actual subscribe not a dummy task
    let shell_sender_ = shell_sender.clone();
    subscription_task.clone().start(
        async move {
            loop {
                msleep(750).await;
                let line = String::from("This is a single line dummy message");
                if shell_sender_.send(vec![line]).await.is_err() {
                    break;
                }
                msleep(750).await;
                let line0 = String::from("This is the first line of a multiline dummy message");
                let line1 = String::from("This is the second line of a multiline dummy message");
                if shell_sender_.send(vec![line0, line1]).await.is_err() {
                    break;
                }
            }
            Ok(())
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => println!("Failed starting dnet subs task: {e}"),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    *subscription_active = true;
}

/// Auxiliary function to define the unsubscribe command handling.
async fn handle_unsubscribe(subscription_active: &mut bool, subscription_task: &StoppableTaskPtr) {
    if !*subscription_active {
        println!("Subscription is already inactive!")
    }
    subscription_task.stop().await;
    *subscription_active = false;
}

/// Auxiliary function to define the scan command handling.
async fn handle_scan(drk: &Drk, subscription_active: &bool, parts: &[&str]) {
    if *subscription_active {
        println!("Subscription is already active!");
        return
    }

    // Check correct command structure
    if parts.len() != 1 && parts.len() != 3 {
        println!("Malformed `scan` command");
        return
    }

    // Check if reset was requested
    if parts.len() == 3 {
        if parts[1] != "--reset" {
            println!("Malformed `scan` command");
            println!("Usage: scan --reset {{height}}");
            return
        }

        let height = match u32::from_str(parts[2]) {
            Ok(h) => h,
            Err(e) => {
                println!("Invalid reset height: {e:?}");
                return
            }
        };

        if let Err(e) = drk.reset_to_height(height) {
            println!("Failed during wallet reset: {e:?}");
            return
        }
    }

    if let Err(e) = drk.scan_blocks().await {
        println!("Failed during scanning: {e:?}");
        return
    }
    println!("Finished scanning blockchain");
}
