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

use std::{sync::Arc, time::Duration};

use smol::{future::Future, Executor, Timer};

/// Condition variable which allows a task to block until woken up
pub mod condvar;
pub use condvar::CondVar;

/// Implementation of async background task spawning which are stoppable
/// using channel signalling.
pub mod stoppable_task;
pub use stoppable_task::{StoppableTask, StoppableTaskPtr};

/// Simple broadcast (publish-subscribe) class
pub mod publisher;
pub use publisher::{Publisher, PublisherPtr, Subscription};

/// Async timeout implementations
pub mod timeout;
pub use timeout::io_timeout;

/// Thread priority setting
pub mod thread_priority;

pub type ExecutorPtr = Arc<Executor<'static>>;

/// Sleep for any number of seconds.
pub async fn sleep(seconds: u64) {
    Timer::after(Duration::from_secs(seconds)).await;
}

pub async fn sleep_forever() {
    loop {
        sleep(100000000).await
    }
}

/// Sleep for any number of milliseconds.
pub async fn msleep(millis: u64) {
    Timer::after(Duration::from_millis(millis)).await;
}

/// Run a task until it has fully completed, irrespective of whether the parent task still exists.
pub async fn run_until_completion<'a, R: Send + 'a, F: Future<Output = R> + Send + 'a>(
    func: F,
    executor: Arc<Executor<'a>>,
) -> R {
    let (sender, recv_queue) = smol::channel::bounded::<R>(1);
    executor
        .spawn(async move {
            let result = func.await;
            // We ignore this result: an error would mean the parent task has been cancelled,
            // which is valid behavior.
            let _ = sender.send(result).await;
        })
        .detach();
    // This should never panic because it would mean the detached task has not completed.
    recv_queue.recv().await.expect("Run until completion task failed")
}
