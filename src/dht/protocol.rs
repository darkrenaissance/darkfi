use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use log::{debug, error};

use crate::{
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

use super::{
    messages::{KeyRequest, KeyResponse, LookupMapRequest, LookupMapResponse, LookupRequest},
    DhtPtr,
};

pub struct Protocol {
    channel: ChannelPtr,
    notify_queue_sender: async_channel::Sender<KeyResponse>,
    req_sub: MessageSubscription<KeyRequest>,
    resp_sub: MessageSubscription<KeyResponse>,
    lookup_sub: MessageSubscription<LookupRequest>,
    lookup_map_sub: MessageSubscription<LookupMapRequest>,
    jobsman: ProtocolJobsManagerPtr,
    dht: DhtPtr,
    p2p: P2pPtr,
}

impl Protocol {
    pub async fn init(
        channel: ChannelPtr,
        notify_queue_sender: async_channel::Sender<KeyResponse>,
        dht: DhtPtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding Protocol to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<KeyRequest>().await;
        msg_subsystem.add_dispatch::<KeyResponse>().await;
        msg_subsystem.add_dispatch::<LookupRequest>().await;
        msg_subsystem.add_dispatch::<LookupMapRequest>().await;

        let req_sub = channel.subscribe_msg::<KeyRequest>().await?;
        let resp_sub = channel.subscribe_msg::<KeyResponse>().await?;
        let lookup_sub = channel.subscribe_msg::<LookupRequest>().await?;
        let lookup_map_sub = channel.subscribe_msg::<LookupMapRequest>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            notify_queue_sender,
            req_sub,
            resp_sub,
            lookup_sub,
            lookup_map_sub,
            jobsman: ProtocolJobsManager::new("Protocol", channel),
            dht,
            p2p,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!("Protocol::handle_receive_request() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let req = match self.req_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("Protocol::handle_receive_request(): recv fail: {}", e);
                    continue
                }
            };

            let req_copy = (*req).clone();
            debug!("Protocol::handle_receive_request(): req: {:?}", req_copy);

            {
                let dht = &mut self.dht.write().await;
                if dht.seen.contains_key(&req_copy.id) {
                    debug!(
                        "Protocol::handle_receive_request(): We have already seen this request."
                    );
                    continue
                }

                dht.seen.insert(req_copy.id, Utc::now().timestamp());
            }

            let daemon = self.dht.read().await.id;
            if daemon != req_copy.to {
                if let Err(e) =
                    self.p2p.broadcast_with_exclude(req_copy.clone(), &exclude_list).await
                {
                    error!("Protocol::handle_receive_response(): p2p broadcast fail: {}", e);
                };
                continue
            }

            match self.dht.read().await.map.get(&req_copy.key) {
                Some(value) => {
                    let response =
                        KeyResponse::new(daemon, req_copy.from, req_copy.key, value.clone());
                    debug!("Protocol::handle_receive_request(): sending response: {:?}", response);
                    if let Err(e) = self.channel.send(response).await {
                        error!("Protocol::handle_receive_request(): p2p broadcast of response failed: {}", e);
                    };
                }
                None => {
                    error!("Protocol::handle_receive_request(): Requested key doesn't exist locally: {}", req_copy.key);
                }
            }
        }
    }

    async fn handle_receive_response(self: Arc<Self>) -> Result<()> {
        debug!("Protocol::handle_receive_response() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let resp = match self.resp_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("Protocol::handle_receive_response(): recv fail: {}", e);
                    continue
                }
            };

            let resp_copy = (*resp).clone();
            debug!("Protocol::handle_receive_response(): resp: {:?}", resp_copy);

            {
                let dht = &mut self.dht.write().await;
                if dht.seen.contains_key(&resp_copy.id) {
                    debug!(
                        "Protocol::handle_receive_request(): We have already seen this request."
                    );
                    continue
                }

                dht.seen.insert(resp_copy.id, Utc::now().timestamp());
            }

            if self.dht.read().await.id != resp_copy.to {
                if let Err(e) =
                    self.p2p.broadcast_with_exclude(resp_copy.clone(), &exclude_list).await
                {
                    error!("Protocol::handle_receive_response(): p2p broadcast fail: {}", e);
                };
                continue
            }

            self.notify_queue_sender.send(resp_copy.clone()).await?;
        }
    }

    async fn handle_receive_lookup_request(self: Arc<Self>) -> Result<()> {
        debug!("Protocol::handle_receive_lookup_request() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let req = match self.lookup_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("Protocol::handle_receive_lookup_request(): recv fail: {}", e);
                    continue
                }
            };

            let req_copy = (*req).clone();
            debug!("Protocol::handle_receive_lookup_request(): req: {:?}", req_copy);

            if !(0..=1).contains(&req_copy.req_type) {
                debug!("Protocol::handle_receive_lookup_request(): Unknown request type.");
                continue
            }

            {
                let dht = &mut self.dht.write().await;
                if dht.seen.contains_key(&req_copy.id) {
                    debug!(
                        "Protocol::handle_receive_request(): We have already seen this request."
                    );
                    continue
                }

                dht.seen.insert(req_copy.id, Utc::now().timestamp());
            }

            let result = match req_copy.req_type {
                0 => self.dht.write().await.lookup_insert(req_copy.key, req_copy.daemon),
                _ => self.dht.write().await.lookup_remove(req_copy.key, req_copy.daemon),
            };

            if let Err(e) = result {
                error!("Protocol::handle_receive_lookup_request(): request action failed: {}", e);
                continue
            };

            if let Err(e) = self.p2p.broadcast_with_exclude(req_copy, &exclude_list).await {
                error!("Protocol::handle_receive_lookup_request(): p2p broadcast fail: {}", e);
            };
        }
    }

    async fn handle_receive_lookup_map_request(self: Arc<Self>) -> Result<()> {
        debug!("Protocol::handle_receive_lookup_map_request() [START]");
        loop {
            let req = match self.lookup_map_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("Protocol::handle_receive_lookup_map_request(): recv fail: {}", e);
                    continue
                }
            };

            debug!("Protocol::handle_receive_lookup_map_request(): req: {:?}", req);

            {
                let dht = &mut self.dht.write().await;
                if dht.seen.contains_key(&req.id) {
                    debug!(
                        "Protocol::handle_receive_lookup_map_request(): We have already seen this request."
                    );
                    continue
                }

                dht.seen.insert(req.id, Utc::now().timestamp());
            }

            // Extra validations can be added here.
            let lookup = self.dht.read().await.lookup.clone();
            let response = LookupMapResponse::new(lookup);
            if let Err(e) = self.channel.send(response).await {
                error!("Protocol::handle_receive_lookup_map_request() channel send fail: {}", e);
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for Protocol {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("Protocol::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_response(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_lookup_request(), executor.clone())
            .await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_lookup_map_request(), executor.clone())
            .await;
        debug!("Protocol::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Protocol"
    }
}
