use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use secp256k1::key::SecretKey;
use bitcoin::util::ecdsa::{PrivateKey, PublicKey as BitcoinPubKey};
use bitcoin::util::address::Address;

use bitcoin::network::constants::Network;
use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};
use crate::blockchain::{rocks::columns, RocksColumn, CashierKeypair, CashierStore};
use crate::{serial::deserialize, serial::serialize, Error, Result};
use crate::wallet::{WalletDb, WalletPtr};
use std::net::SocketAddr;
use async_std::sync::Arc;
use async_executor::Executor;
use log::*;

#[repr(u8)]
enum CashierError {
    NoError,
    UpdateIndex,
}

#[repr(u8)]
enum CashierCommand {
    GetDBTC,
    GetBTC,
}

pub struct BitcoinKeys {
    secret_key: SecretKey,
    bitcoin_private_key: PrivateKey,
    pub bitcoin_public_key: BitcoinPubKey,
    pub pub_address: Address,
}
impl BitcoinKeys {
    pub fn new(

    ) -> Result<BitcoinKeys> {

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

        // Use Testnet
        let bitcoin_private_key = PrivateKey::new(secret_key, Network::Testnet);

        let bitcoin_public_key = BitcoinPubKey::from_private_key(&context, &bitcoin_private_key);

        let pub_address = Address::p2pkh(&bitcoin_public_key, Network::Testnet);

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
    wallet: Arc<WalletDb>,
}

impl CashierService {
    pub fn new(
        addr: SocketAddr,
        rocks: RocksColumn<columns::CashierKeys>,
        wallet: Arc<WalletDb>,
    )-> Result<Arc<CashierService>> {
        let cashierstore = CashierStore::new(rocks)?;

        Ok(Arc::new(CashierService {
            cashierstore,
            addr,
            wallet,
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
                let zkpub = request.get_payload();

                // Generate bitcoin Address
                let btc_keys = BitcoinKeys::new().unwrap();
                let deposit_address = btc_keys.get_deposit_address();

                // add to slabstore
                let error = cashierstore.put(deserialize(&zkpub)?)?;

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);
                // if let None = error {
                //     reply.set_error(CashierError::UpdateIndex as u32);
                // }
                // send reply
                send_queue.send((peer, reply)).await?;

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
        //self.sync().await?;

        Ok(())
    }

    pub async fn get_keys(&mut self, index: jubjub::SubgroupPoint) -> Result<Option<CashierKeypair>> {
        let rep = self
            .protocol
            .request(
                CashierCommand::GetDBTC as u8,
                serialize(&index),
                &handle_error,
            )
            .await?;

        if let Some(keys) = rep {
            let keys: CashierKeypair = deserialize(&keys)?;
            //self.gateway_slabs_sub_s.send(slab.clone()).await?;
            self.cashierstore.put(keys.clone())?;
            return Ok(Some(keys));
        }
        Ok(None)
    }

    // pub async fn put_keys(&mut self, mut keys: CashierKeys) -> Result<()> {
    //     loop {
    //         let last_index = self.sync().await?;
    //         //keys.set_index(last_index + 1);
    //         let keys = serialize(&keys);

    //         let rep = self
    //             .protocol
    //             .request(CashierCommand::PutSlab as u8, slab.clone(), &handle_error)
    //             .await?;

    //         if let Some(_) = rep {
    //             break;
    //         }
    //     }
    //     Ok(())
    // }

    pub fn get_cashierstore(&self) -> Arc<CashierStore> {
        self.cashierstore.clone()
    }

}

fn handle_error(status_code: u32) {
    match status_code {
        1 => {
            warn!("Reply has an Error: Index is not updated");
        }
        2 => {
            warn!("Reply has an Error: Index Not Exist");
        }
        _ => {}
    }
}
