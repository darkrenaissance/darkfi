use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll},
};

/// Condition variable which allows a task to block until woken up
pub struct CondVar {
    is_active: AtomicBool,
}

impl CondVar {
    pub fn new() -> Self {
        Self { is_active: AtomicBool::new(false) }
    }

    /// Wakeup the waiting task. Subsequent calls to this do nothing until `wait()` is called.
    pub fn notify(&mut self) {
        self.is_active.store(true, Ordering::Relaxed)
    }

    /// Reset the condition variable and wait for a notification
    pub async fn wait(&self) -> CondVarWait<'_> {
        self.is_active.store(false, Ordering::SeqCst);
        CondVarWait { condvar: self }
    }

    fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }
}

pub struct CondVarWait<'a> {
    condvar: &'a CondVar,
}

impl<'a> Future for CondVarWait<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.condvar.is_active() {
            true => Poll::Ready(()),
            false => Poll::Pending,
        }
    }
}
