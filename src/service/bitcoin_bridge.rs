use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use secp256k1::key::{SecretKey, PublicKey};
use bitcoin::util::ecdsa::{PrivateKey, PublicKey as BitcoinPubKey};
use bitcoin::util::{address::Payload, address::Address};
// Use p2pkh for 1st iteration
use bitcoin::hash_types::PubkeyHash;
use bitcoin::network::constants::Network;
use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};
use crate::blockchain::{rocks::columns, RocksColumn, CashierKeypair, CashierStore};
use crate::{serial::deserialize, serial::serialize, Error, Result};
use std::net::SocketAddr;
use async_std::sync::Arc;
use async_executor::Executor;


pub struct CashierKeys {
    secret_key: SecretKey,
    bitcoin_private_key: PrivateKey,
    pub bitcoin_public_key: BitcoinPubKey,
    pub pub_address: Address,
}
impl CashierKeys {
    pub fn new(

    ) -> Result<CashierKeys> {

        let context = secp256k1::Secp256k1::new();

        // Probably not good enough for release
        let rand: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let rand_hex = hex::encode(rand);

        // Generate simple byte array from rand
        let data_slice: &[u8] = rand_hex.as_bytes();

        let secret_key = SecretKey::from_slice(&hex::decode(data_slice).unwrap()).unwrap();

        //let public_key = PublicKey::from_secret_key(&context, &secret_key);

        // Use mainnet
        let bitcoin_private_key = PrivateKey::new(secret_key, Network::Bitcoin);

        let bitcoin_public_key = BitcoinPubKey::from_private_key(&context, &bitcoin_private_key);

        let pub_address = Address::p2pkh(&bitcoin_public_key, Network::Bitcoin);

        Ok(Self {
            secret_key,
            bitcoin_private_key,
            bitcoin_public_key,
            pub_address,
        })
    }
    pub fn get_deposit_address(&self) -> &Address {
        &self.pub_address
    }
}

pub struct CashierService {
    addr: SocketAddr,
    cashierstore: Arc<CashierStore>,
}

impl CashierService {
    pub fn new(
        addr: SocketAddr,
        rocks: RocksColumn<columns::CashierKeys>,
    )-> Result<Arc<CashierService>> {
        let cashierstore = CashierStore::new(rocks)?;

        Ok(Arc::new(CashierService {
            cashierstore,
            addr,
        }))
    }
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let service_name = String::from("CASHIER DAEMON");

        let mut protocol = RepProtocol::new(self.addr.clone(), service_name.clone());

        let (send, recv) = protocol.start().await?;

        let handle_request_task = executor.spawn(self.handle_request_loop(
            send.clone(),
            recv.clone(),
            executor.clone(),
        ));

        protocol.run(executor.clone()).await?;

        let _ = handle_request_task.cancel().await;
        Ok(())
    }

    async fn handle_request_loop(
        self: Arc<Self>,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        recv_queue: async_channel::Receiver<(PeerId, Request)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        loop {
            match recv_queue.recv().await {
                Ok(msg) => {
                    let cashierstore = self.cashierstore.clone();
                    let _ = executor
                        .spawn(Self::handle_request(
                            msg,
                            cashierstore,
                            send_queue.clone(),
                        ))
                        .detach();
                }
                Err(_) => {
                    break;
                }
            }
        }
        Ok(())
    }
    async fn handle_request(
        msg: (PeerId, Request),
        cashierstore: Arc<CashierStore>,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
    ) -> Result<()> {
        let request = msg.1;
        let peer = msg.0;
        match request.get_command() {
            0 => {
                // Exchange zk_pubkey for bitcoin address

            }
            1 => {
                // Withdraw
            }
            _ => {
                return Err(Error::ServicesError("received wrong command"));
            }
        }
        Ok(())
    }
}

pub struct CashierClient {
    protocol: ReqProtocol,
    cashierstore: Arc<CashierStore>,
}

impl CashierClient {
    pub fn new(addr: SocketAddr, rocks: RocksColumn<columns::CashierKeys>) -> Result<Self> {
        let protocol = ReqProtocol::new(addr, String::from("CASHIER CLIENT"));

        let cashierstore = CashierStore::new(rocks)?;

        Ok(CashierClient {
            protocol,
            cashierstore,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;
        self.sync().await?;

        Ok(())
    }

    pub async fn get_keys(&mut self, index: u64) -> Result<Option<CashierKeypair>> {
        let rep = self
            .protocol
            .request(
                //CashierCommand::GetKeys as u8,
                serialize(&index),
                &handle_error,
            )
            .await?;

        if let Some(keys) = rep {
            let keys: CashierKeys = deserialize(&keys)?;
            //self.gateway_slabs_sub_s.send(slab.clone()).await?;
            self.cashierstore.put(keys.clone())?;
            return Ok(Some(keys));
        }
        Ok(None)
    }

    pub async fn put_keys(&mut self, mut keys: CashierKeys) -> Result<()> {
        loop {
            let last_index = self.sync().await?;
            //keys.set_index(last_index + 1);
            let keys = serialize(&keys);

            let rep = self
                .protocol
                .request(GatewayCommand::PutSlab as u8, slab.clone(), &handle_error)
                .await?;

            if let Some(_) = rep {
                break;
            }
        }
        Ok(())
    }

    pub async fn get_last_index(&mut self) -> Result<u64> {
        let rep = self
            .protocol
            .request(CashierCommand::GetLastIndex as u8, vec![], &handle_error)
            .await?;
        if let Some(index) = rep {
            return Ok(deserialize(&index)?);
        }
        Ok(0)
    }

    pub fn get_cashierstore(&self) -> Arc<SlabStore> {
        self.cashierstore.clone()
    }

}
