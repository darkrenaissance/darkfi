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

use std::{
    error::Error,
    fmt,
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use pin_project_lite::pin_project;
use smol::Timer;

/// Awaits an I/O future or times out after a duration of time.
///
/// If you want to await a non I/O future consider using
/// `timeout()` instead.
///
/// # Examples
///
/// ```no_run
/// # fn main() -> std::io::Result<()> { smol::block_on(async {
/// #
/// use std::time::Duration;
/// use std::io;
///
/// io_timeout(Duration::from_secs(5), async {
///     let stdin = io::stdin();
///     let mut line = String::new();
///     let n = stdin.read_line(&mut line)?;
///     Ok(())
/// })
/// .await?;
/// #
/// # Ok(()) }) }
pub async fn io_timeout<F, T>(dur: Duration, f: F) -> io::Result<T>
where
    F: Future<Output = io::Result<T>>,
{
    Timeout { timeout: Timer::after(dur), future: f }.await
}

pin_project! {
    #[derive(Debug)]
    pub struct Timeout<F, T>
    where
        F: Future<Output = io::Result<T>>,
    {
        #[pin]
        future: F,
        #[pin]
        timeout: Timer,
    }
}

impl<F, T> Future for Timeout<F, T>
where
    F: Future<Output = io::Result<T>>,
{
    type Output = io::Result<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.future.poll(cx) {
            Poll::Pending => {}
            other => return other,
        }

        if this.timeout.poll(cx).is_ready() {
            let err = Err(io::Error::new(io::ErrorKind::TimedOut, "future timed out"));
            Poll::Ready(err)
        } else {
            Poll::Pending
        }
    }
}

/// Awaits a future or times out after a duration of time.
///
/// If you want to await an I/O future consider using
/// `io_timeout` instead.
///
/// # Examples
///
/// ```
/// # fn main() -> std::io::Result<()> { smol::block_on(async {
/// #
/// use std::time::Duration;
/// use smol::future;
///
/// let never = future::pending::<()>();
/// let dur = Duration::from_millis(5);
/// assert!(timeout(dur, never).await.is_err());
/// #
/// # Ok(()) }) }
/// ```
pub async fn timeout<F, T>(dur: Duration, f: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    TimeoutFuture::new(f, dur).await
}

pin_project! {
    /// A future that times out after a duration of time.
    pub struct TimeoutFuture<F> {
        #[pin]
        future: F,
        #[pin]
        delay: Timer,
    }
}

impl<F> TimeoutFuture<F> {
    #[allow(dead_code)]
    pub(super) fn new(future: F, dur: Duration) -> TimeoutFuture<F> {
        TimeoutFuture { future, delay: Timer::after(dur) }
    }
}

impl<F: Future> Future for TimeoutFuture<F> {
    type Output = Result<F::Output, TimeoutError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.future.poll(cx) {
            Poll::Ready(v) => Poll::Ready(Ok(v)),
            Poll::Pending => match this.delay.poll(cx) {
                Poll::Ready(_) => Poll::Ready(Err(TimeoutError { _private: () })),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

/// An error returned when a future times out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeoutError {
    _private: (),
}

impl Error for TimeoutError {}

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "future has timed out".fmt(f)
    }
}
