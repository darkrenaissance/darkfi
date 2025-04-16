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
    wakers: Vec<Waker>,
}

impl CondVar {
    pub fn new() -> Self {
        Self { state: Mutex::new(CondVarState { is_awake: false, wakers: Vec::new() }) }
    }

    /// Wakeup the waiting task. Subsequent calls to this do nothing until `wait()` is called.
    pub fn notify(&self) {
        let wakers = {
            let mut state = self.state.lock().unwrap();
            state.is_awake = true;
            std::mem::take(&mut state.wakers)
        };
        // Notify the executor that the pending future from wait() is to be polled again.
        for waker in wakers {
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

        if state.is_awake {
            return Poll::Ready(())
        }

        let cx_waker = cx.waker();

        // Do we have any wakers in the list?
        if !state.wakers.iter().any(|w| cx_waker.will_wake(w)) {
            // Add our waker
            state.wakers.push(cx_waker.clone())
        }

        Poll::Pending
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
            let t = executor_.spawn(async move {
                // Waits here until notify() is called
                cv_.wait().await;
            });

            // Allow above code to continue
            cv.notify();

            t.await;
        }))
    }

    #[test]
    fn condvar_reset() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv_ = cv.clone();
            let t = executor_.spawn(async move {
                cv_.wait().await;
            });

            // #1 send signal
            cv.notify();
            // Multiple calls to notify do nothing until we call reset()
            cv.notify();

            t.await;

            // Without calling reset(), then the wait() will return instantly
            cv.reset();

            let cv_ = cv.clone();
            let t = executor_.spawn(async move {
                cv_.wait().await;
            });

            // #2 send signal again
            cv.notify();

            t.await;
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
            let t1 = executor_.spawn(async move { cv2.wait().await });
            let t2 = executor_.spawn(async move { cv3.wait().await });

            // Allow above code to continue
            cv.notify();

            t1.await;
            t2.await;
        }))
    }

    #[test]
    fn condvar_wait_after_notify() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv2 = cv.clone();
            let t = executor_.spawn(async move { cv2.wait().await });

            cv.notify();
            t.await;

            // Should complete immediately
            let cv2 = cv.clone();
            let t = executor_.spawn(async move { cv2.wait().await });
            t.await;
        }))
    }

    #[test]
    fn condvar_drop() {
        let executor = Arc::new(Executor::new());
        let executor_ = executor.clone();
        smol::block_on(executor.run(async move {
            let cv = Arc::new(CondVar::new());

            let cv_ = cv.clone();
            let t = executor_.spawn(async move {
                select! {
                    () = cv_.wait().fuse() => (),
                    () = (async {}).fuse() => ()
                }

                // The above future was dropped and we make a new one
                cv_.wait().await
            });

            // Allow above code to continue
            cv.notify();
            t.await;
        }))
    }
}
