use async_executor::Executor;
use futures::Future;
use futures::FutureExt;
use std::sync::Arc;

pub type StoppableTaskPtr = Arc<StoppableTask>;

pub struct StoppableTask {
    stop_send: async_channel::Sender<()>,
    stop_recv: async_channel::Receiver<()>,
}

impl StoppableTask {
    pub fn new() -> Arc<Self> {
        let (stop_send, stop_recv) = async_channel::unbounded();
        Arc::new(Self {
            stop_send,
            stop_recv,
        })
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
