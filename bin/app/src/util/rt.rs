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

use async_channel::{Receiver, Sender};
use parking_lot::Mutex as SyncMutex;
use smol::Task;
use std::{sync::Arc, thread};

use crate::util::spawn_thread;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "rt", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "rt", $($arg)*); } }

pub type ExecutorPtr = Arc<smol::Executor<'static>>;

pub struct AsyncRuntime {
    name: &'static str,
    signal: Sender<()>,
    shutdown: Receiver<()>,
    exec_threadpool: SyncMutex<Vec<thread::JoinHandle<()>>>,
    ex: ExecutorPtr,
    tasks: SyncMutex<Vec<Task<()>>>,
}

impl AsyncRuntime {
    pub fn new(ex: ExecutorPtr, name: &'static str) -> Self {
        let (signal, shutdown) = async_channel::unbounded::<()>();

        Self {
            name,
            signal,
            shutdown,
            exec_threadpool: SyncMutex::new(vec![]),
            ex,
            tasks: SyncMutex::new(vec![]),
        }
    }

    pub fn start(&self) {
        let n_threads = thread::available_parallelism().unwrap().get();
        self.start_with_count(n_threads);
    }

    pub fn start_with_count(&self, n_threads: usize) {
        let mut exec_threadpool = Vec::with_capacity(n_threads);
        // N executor threads
        for i in 0..n_threads {
            let shutdown = self.shutdown.clone();
            let ex = self.ex.clone();

            let name = format!("{}-{}", self.name, i);
            let handle = spawn_thread(name, move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            });
            exec_threadpool.push(handle);
        }
        *self.exec_threadpool.lock() = exec_threadpool;
        info!(target: "rt", "[{}] Started runtime [{n_threads} threads]", self.name);
    }

    pub fn push_task(&self, task: Task<()>) {
        self.tasks.lock().push(task);
    }

    pub fn stop(&self) {
        let exec_threadpool = std::mem::take(&mut *self.exec_threadpool.lock());

        d!("[{}] Stopping async runtime...", self.name);
        // Just drop all the tasks without waiting for them to finish.
        self.tasks.lock().clear();

        for _ in &exec_threadpool {
            self.signal.try_send(()).unwrap();
        }

        for handle in exec_threadpool {
            handle.join().unwrap();
        }

        t!("[{}] Stopped runtime", self.name);
    }
}
