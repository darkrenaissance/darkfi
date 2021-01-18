use log::*;
use std::sync::atomic::Ordering;

use crate::net::messages as net;
use crate::utility::{get_current_time, AddrsStorage, Clock, ConnectionsMap};
use crate::Result;

// Clients send repeated pings. Servers only respond with pong.
pub async fn repeat_ping(send_sx: async_channel::Sender<net::Message>, clock: Clock) -> Result<()> {
    debug!("ping process");
    loop {
        // Send ping
        send_sx.send(net::Message::Ping).await?;
        debug!("send Message::Ping");
        clock.store(get_current_time(), Ordering::Relaxed);

        net::sleep(5).await;
    }
}

pub async fn protocol(
    message: net::Message,
    stored_addrs: &AddrsStorage,
    send_sx: &async_channel::Sender<net::Message>,
    clock: Option<&Clock>,
    connections: ConnectionsMap,
) -> Result<()> {
    match message {
        net::Message::Ping => {
            send_sx.send(net::Message::Pong).await?;
        }
        net::Message::Pong => {
            if let Some(clock) = clock {
                let current_time = get_current_time();
                let elapsed = current_time - clock.load(Ordering::Relaxed);
                info!("Ping time: {} ms", elapsed);
            }
        }
        net::Message::GetAddrs(_message) => {
            info!("received GetAddrMessage");
            send_sx
                .send(net::Message::Addrs(net::AddrsMessage {
                    addrs: stored_addrs.lock().await.to_vec(),
                }))
                .await?;
        }
        net::Message::Addrs(message) => {
            info!("received AddrMessage");
            let mut stored_addrs = stored_addrs.lock().await;
            for addr in message.addrs {
                if stored_addrs.contains(&addr) {
                    continue;
                }
                info!("Added new address to storage {}", addr.to_string());
                stored_addrs.push(addr);
            }
        }
        net::Message::Sync => {
            info!("received SyncMessage");
            /*send_sx
            .send(net::Message::Inv(net::InvMessage {
                slabs_hash: slabman.get_slabs_hash(),
            }))
            .await?;*/
        }
        net::Message::Inv(_message) => {
            info!("received inv message");
            /*
            let mut list_of_hash: Vec<net::CiphertextHash> = vec![];
            for slab in message.slabs_hash.iter() {
                /*if !slabman.get_slabs_hash().contains(slab) {
                    list_of_hash.push(slab.clone());
                }*/
            }
            send_sx
                .send(net::Message::GetSlabs(net::GetSlabsMessage {
                    slabs_hash: list_of_hash,
                }))
                .await?;
            */
        }

        net::Message::GetSlabs(message) => {
            info!("received GetSlabs message.");
            for _slab_hash in message.slabs_hash {
                /*let slab = slabman.get_slab(&slab_hash);
                if let Some(slab) = slab {
                    send_sx.send(net::Message::Slab(slab.clone())).await?;
                }*/
            }
        }
        net::Message::Slab(message) => {
            let _slab = net::SlabMessage {
                nonce: message.nonce,
                ciphertext: message.ciphertext.clone(),
            };

            // TODO:  it doesn't have to send inv message to the connection which sent the slab.
            for (a, _send) in connections.lock().await.iter() {
                println!("send to {:?}", a);
                /*send.send(net::Message::Inv(net::InvMessage {
                    slabs_hash: vec![slab.cipher_hash()],
                }))
                .await?;*/
            }
        }
        _ => {}
    }
    Ok(())
}
