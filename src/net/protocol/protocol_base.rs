use async_trait::async_trait;
use smol::Executor;
use std::sync::Arc;

pub type ProtocolBasePtr = Arc<dyn 'static + ProtocolBase + Send + Sync>;

#[async_trait]
pub trait ProtocolBase {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>);
}
