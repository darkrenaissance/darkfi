// Adapter class goes here
//use crate::rpc::jsonserver::JsonRpcInterface;
use std::sync::Arc;

// Dummy adapter for now
pub struct RpcAdapter {}

impl RpcAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub async fn get_info() {}

    pub async fn key_gen() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}

//let stop_send = self.stop_send.clone();
//io.add_method("stop", move |_| {
//    let stop_send = stop_send.clone();
//    async move {
//        RpcAdapter::stop().await;
//        let _ = stop_send.send(()).await;
//        Ok(jsonrpc_core::Value::Null)
//    }
//});

//let stop_send = self.stop_send.clone();
//pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
//    Ok(self.stop_recv.recv().await?)
//}
//let self2 = self2.clone();
//async move {
//    Ok(json!({
//        "started": *self2.started.lock().await,
//        "connections": self2.p2p.connections_count().await
//    }))

//pub async fn serve(self: Arc<Self>, mut req: Request) ->
// http_types::Result<Response> {    info!("RPC serving {}", req.url());

//    let request = req.body_string().await?;

//    let mut res = Response::new(StatusCode::Ok);
//    res.insert_header("Content-Type", "text/plain");
//    res.set_body(response);
//    Ok(res)
//}

//pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
//    Ok(self.stop_recv.recv().await?)
//}
