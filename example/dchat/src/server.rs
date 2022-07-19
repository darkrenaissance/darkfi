use futures::{io::WriteHalf, AsyncRead, AsyncWrite, AsyncWriteExt};
use ringbuffer::{RingBufferExt, RingBufferWrite};
use std::net::SocketAddr;

use darkfi::{net::P2pPtr, system::SubscriberPtr, Result};

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};

pub struct DchatserverConnection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    // server stream
    write_stream: WriteHalf<C>,
    peer_address: SocketAddr,
    // msgs
    dchatmsgs_buffer: DchatmsgsBuffer,
    // p2p
    p2p: P2pPtr,
    senders: SubscriberPtr<Dchatmsg>,
    subscriber_id: u64,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> DchatserverConnection<C> {
    pub fn new(
        write_stream: WriteHalf<C>,
        peer_address: SocketAddr,
        dchatmsgs_buffer: DchatmsgsBuffer,
        p2p: P2pPtr,
        senders: SubscriberPtr<Dchatmsg>,
        subscriber_id: u64,
    ) -> Self {
        Self { write_stream, peer_address, dchatmsgs_buffer, p2p, senders, subscriber_id }
    }

    async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        //debug!("Sent {}", message);
        Ok(())
    }

    async fn update(&mut self, line: String) -> Result<()> {
        // read from STDIN??
        let mut tokens = line.split_ascii_whitespace();
        Ok(())
    }

    async fn on_receive_dchatmsg(&mut self, message: &str, target: &str) -> Result<()> {
        let protocol_msg = Dchatmsg { message: message.to_string() };

        {
            (*self.dchatmsgs_buffer.lock().await).push(protocol_msg.clone())
        }

        self.senders.notify_with_exclude(protocol_msg.clone(), &[self.subscriber_id]).await;

        //debug!(target: "ircd", "PRIVMSG to be sent: {:?}", protocol_msg);
        self.p2p.broadcast(protocol_msg).await?;

        Ok(())
    }
}
