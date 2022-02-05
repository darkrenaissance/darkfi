use async_trait::async_trait;
use smol::Executor;
use std::sync::Arc;

use crate::error::Result;

pub type ProtocolBasePtr = Arc<dyn ProtocolBase + Send + Sync>;

#[async_trait]
pub trait ProtocolBase {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()>;
}
