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

use std::sync::Arc;

use smol::{
    channel,
    future::{self, Future},
    Executor,
};

pub type StoppableTaskPtr = Arc<StoppableTask>;

#[derive(Debug)]
pub struct StoppableTask {
    // NOTE: we could send the error code from stop() instead of having it specified in start()
    // but then that would introduce lifetimes to the entire struct.
    stop_send: channel::Sender<()>,
    stop_recv: channel::Receiver<()>,
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
        let (stop_send, stop_recv) = channel::bounded(1);
        Arc::new(Self { stop_send, stop_recv })
    }

    /// Stops the task
    pub async fn stop(&self) {
        // Ignore any errors from this send
        let _ = self.stop_send.send(()).await;
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
        executor
            .spawn(async move {
                let stop_fut = async {
                    let _ = self.stop_recv.recv().await;
                    Err(stop_value)
                };

                let result = future::or(main, stop_fut).await;
                stop_handler(result).await;
            })
            .detach();
    }
}
