use std::sync::Arc;

use async_trait::async_trait;
use smol::Executor;

use crate::Result;

pub type ProtocolBasePtr = Arc<dyn ProtocolBase + Send + Sync>;

#[async_trait]
pub trait ProtocolBase {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()>;

    fn name(&self) -> &'static str;
}
