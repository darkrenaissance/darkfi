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

use log::trace;
use rand::{rngs::OsRng, Rng};
use smol::{
    future::{self, Future},
    Executor,
};
use std::sync::Arc;

use super::CondVar;

pub type StoppableTaskPtr = Arc<StoppableTask>;

pub struct StoppableTask {
    /// Used to signal to the main running process that it should stop.
    signal: CondVar,
    /// When we call `stop()`, we wait until the process is finished. This is used to prevent
    /// `stop()` from exiting until the task has closed.
    barrier: CondVar,

    /// Used so we can keep StoppableTask in HashMap/HashSet
    pub task_id: u32,
}

/// A task that can be prematurely stopped at any time.
///
/// ```rust
///     let task = StoppableTask::new();
///     task.clone().start(
///         my_method(),
///         |result| self_.handle_stop(result),
///         Error::MyStopError,
///         executor,
///     );
/// ```
///
/// Then at any time we can call `task.stop()` to close the task.
impl StoppableTask {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { signal: CondVar::new(), barrier: CondVar::new(), task_id: OsRng.gen() })
    }

    /// Starts the task.
    ///
    /// * `main` is a function of the type `async fn foo() -> ()`
    /// * `stop_handler` is a function of the type `async fn handle_stop(result: Result<()>) -> ()`
    /// * `stop_value` is the Error code passed to `stop_handler` when `task.stop()` is called
    pub fn start<'a, MainFut, StopFut, StopFn, Error>(
        self: Arc<Self>,
        main: MainFut,
        stop_handler: StopFn,
        stop_value: Error,
        executor: Arc<Executor<'a>>,
    ) where
        MainFut: Future<Output = std::result::Result<(), Error>> + Send + 'a,
        StopFut: Future<Output = ()> + Send,
        StopFn: FnOnce(std::result::Result<(), Error>) -> StopFut + Send + 'a,
        Error: std::error::Error + Send + 'a,
    {
        // NOTE: we could send the error code from stop() instead of having it specified in start()
        trace!(target: "system::StoppableTask", "Starting task {}", self.task_id);
        // Allow stopping and starting task again.
        // NOTE: maybe we should disallow this with a panic?
        self.signal.reset();
        self.barrier.reset();

        executor
            .spawn(async move {
                // Task which waits for a stop signal
                let stop_fut = async {
                    self.signal.wait().await;
                    trace!(
                        target: "system::StoppableTask",
                        "Stop signal received for task {}",
                        self.task_id
                    );
                    Err(stop_value)
                };

                // Wait on our main task or stop task - whichever finishes first
                let result = future::or(main, stop_fut).await;

                trace!(
                    target: "system::StoppableTask",
                    "Closing task {} with result: {:?}",
                    self.task_id,
                    result
                );

                stop_handler(result).await;
                // Allow `stop()` to finish
                self.barrier.notify();
            })
            .detach();
    }

    /// Stops the task. On completion, guarantees the process has stopped.
    /// Can be called multiple times. After the first call, this does nothing.
    pub async fn stop(&self) {
        trace!(target: "system::StoppableTask", "Stopping task {}", self.task_id);
        self.signal.notify();
        self.barrier.wait().await;
        trace!(target: "system::StoppableTask", "Stopped task {}", self.task_id);
    }

    /// Sends a stop signal and returns immediately. Doesn't guarantee the task
    /// stopped on completion.
    pub fn stop_nowait(&self) {
        trace!(target: "system::StoppableTask", "Stopping task (nowait) {}", self.task_id);
        self.signal.notify();
    }
}

impl std::hash::Hash for StoppableTask {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.task_id.hash(state);
    }
}

impl std::cmp::PartialEq for StoppableTask {
    fn eq(&self, other: &Self) -> bool {
        self.task_id == other.task_id
    }
}

impl std::cmp::Eq for StoppableTask {}

impl Drop for StoppableTask {
    fn drop(&mut self) {
        self.stop_nowait()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::Error, system::sleep_forever};
    use log::warn;

    #[test]
    fn stoppit_mom() {
        let mut cfg = simplelog::ConfigBuilder::new();
        cfg.add_filter_ignore("async_io".to_string());
        cfg.add_filter_ignore("polling".to_string());

        // We check this error so we can execute same file tests in parallel,
        // otherwise second one fails to init logger here.
        if simplelog::TermLogger::init(
            //simplelog::LevelFilter::Info,
            //simplelog::LevelFilter::Debug,
            simplelog::LevelFilter::Trace,
            cfg.build(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )
        .is_err()
        {
            warn!(target: "test_harness", "Logger already initialized");
        }

        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let task = StoppableTask::new();
            task.clone().start(
                // Main process is an infinite loop
                async {
                    sleep_forever().await;
                    unreachable!()
                },
                // Handle stop
                |result| async move {
                    assert!(matches!(result, Err(Error::DetachedTaskStopped)));
                },
                Error::DetachedTaskStopped,
                executor_,
            );
            task.stop().await;
        }))
    }
}
