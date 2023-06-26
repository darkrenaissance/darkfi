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

use async_std::sync::Arc;

use futures::{Future, FutureExt};
use smol::Executor;

pub type StoppableTaskPtr = Arc<StoppableTask>;

#[derive(Debug)]
pub struct StoppableTask {
    stop_send: smol::channel::Sender<()>,
    stop_recv: smol::channel::Receiver<()>,
}

impl StoppableTask {
    pub fn new() -> Arc<Self> {
        let (stop_send, stop_recv) = smol::channel::unbounded();
        Arc::new(Self { stop_send, stop_recv })
    }

    pub async fn stop(&self) {
        // Ignore any errors from this send
        let _ = self.stop_send.send(()).await;
    }

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
                let result = futures::select! {
                    _ = self.stop_recv.recv().fuse() => Err(stop_value),
                    result = main.fuse() => result
                };

                stop_handler(result).await;
            })
            .detach();
    }
}
