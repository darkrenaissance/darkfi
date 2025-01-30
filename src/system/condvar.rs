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

use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

/// Condition variables allow you to block a task while waiting for an event to occur.
/// Condition variables are typically associated with a boolean predicate (a condition).
/// ```rust
///    let cv = Arc::new(CondVar::new());
///
///    let cv_ = cv.clone();
///    executor_
///        .spawn(async move {
///            // Waits here until notify() is called
///            cv_.wait().await;
///            // Check for some condition...
///        })
///        .detach();
///
///    // Allow above code to continue
///    cv.notify();
/// ```
/// After the condition variable is woken up, the user may `wait` again for another `notify`
/// signal by first calling `cv_.reset()`.
pub struct CondVar {
    state: Mutex<CondVarState>,
}

struct CondVarState {
    is_awake: bool,
    waker: Option<Waker>,
}

impl CondVar {
    pub fn new() -> Self {
        Self { state: Mutex::new(CondVarState { is_awake: false, waker: None }) }
    }

    /// Wakeup the waiting task. Subsequent calls to this do nothing until `wait()` is called.
    pub fn notify(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_awake = true;
        // Notify the executor that the pending future from wait() is to be polled again.
        if let Some(waker) = state.waker.take() {
            waker.wake()
        }
    }

    /// Reset the condition variable and wait for a notification
    pub fn wait(&self) -> CondVarWait {
        CondVarWait { state: &self.state }
    }

    /// Reset self ready to wait() again.
    /// The reason this is separate from `wait()` is that usually
    /// on the first `wait()` we want to catch any `notify()` calls that
    /// happened before we started. For example,
    /// ```rust
    /// loop {
    ///     // Wait for signal
    ///     cv.wait().await;
    ///
    ///     // Do stuff...
    ///
    ///     cv.reset();
    /// }
    /// ```
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_awake = false;
    }
}

impl Default for CondVar {
    fn default() -> Self {
        Self::new()
    }
}

/// Awaitable futures object returned by `condvar.wait()`
pub struct CondVarWait<'a> {
    state: &'a Mutex<CondVarState>,
}

impl Future for CondVarWait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();

        // Avoid cloning wherever possible.
        // This code below is equivalent to:
        //
        //     state.waker = Some(cx.waker().clone());
        //
        // However checking whether the waker we have wakes up the same task
        // as the one in the context cx, means we don't have to re-clone if
        // we already have it.
        //
        // It's a minor thing which is basically recommended in the docs on
        // creating pollable futures.
        let new_waker = match state.waker.take() {
            Some(waker) => {
                let cx_waker = cx.waker();
                if cx_waker.will_wake(&waker) {
                    waker
                } else {
                    cx_waker.clone()
                }
            }
            None => cx.waker().clone(),
        };
        state.waker = Some(new_waker);

        match state.is_awake {
            true => Poll::Ready(()),
            false => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{select, FutureExt};
    use smol::Executor;
    use std::sync::Arc;

    #[test]
    fn condvar_test() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv_ = cv.clone();
            executor_
                .spawn(async move {
                    // Waits here until notify() is called
                    cv_.wait().await;
                })
                .detach();

            // Allow above code to continue
            cv.notify();
        }))
    }

    #[test]
    fn condvar_reset() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv_ = cv.clone();
            executor_
                .spawn(async move {
                    cv_.wait().await;
                })
                .detach();

            // #1 send signal
            cv.notify();
            // Multiple calls to notify do nothing until we call reset()
            cv.notify();

            // Without calling reset(), then the wait() will return instantly
            cv.reset();

            let cv_ = cv.clone();
            executor_
                .spawn(async move {
                    cv_.wait().await;
                })
                .detach();

            // #2 send signal again
            cv.notify();
        }))
    }

    #[test]
    fn condvar_double_wait() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv2 = cv.clone();
            let cv3 = cv.clone();
            executor_.spawn(async move { cv2.wait().await }).detach();
            executor_.spawn(async move { cv3.wait().await }).detach();

            // Allow above code to continue
            cv.notify();
        }))
    }

    #[test]
    fn condvar_wait_after_notify() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv2 = cv.clone();
            executor_.spawn(async move { cv2.wait().await }).detach();

            cv.notify();

            // Should complete immediately
            let cv2 = cv.clone();
            executor_.spawn(async move { cv2.wait().await }).detach();
        }))
    }

    #[test]
    fn condvar_drop() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv_ = cv.clone();
            executor_
                .spawn(async move {
                    select! {
                        () = cv_.wait().fuse() => (),
                        () = (async {}).fuse() => ()
                    }

                    // The above future was dropped and we make a new one
                    cv_.wait().await
                })
                .detach();

            // Allow above code to continue
            cv.notify();
        }))
    }
}
