use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

/// Condition variable which allows a task to block until woken up
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

pub struct CondVarWait<'a> {
    state: &'a Mutex<CondVarState>,
}

impl<'a> Future for CondVarWait<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();

        // Avoid cloning wherever possible.
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
