use smol::Executor;
use std::sync::Arc;

pub type ExecutorPtr<'a> = Arc<Executor<'a>>;
