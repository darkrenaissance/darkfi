/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use futures::{
    future::{select, Either},
    pin_mut,
};
use futures_rustls::rustls::{self, version::TLS13};
use quinn::{
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
    ClientConfig, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig, VarInt,
};
use smol::{
    io::{AsyncRead, AsyncWrite},
    lock::OnceCell,
    Timer,
};
use tracing::debug;
use url::Url;

use super::{
    tls::{
        generate_certificate, ClientCertificateVerifier, ServerCertificateVerifier, TLS_DNS_NAME,
    },
    PtListener, PtStream,
};

/// Create QUIC client configuration with our TLS config
fn create_client_config() -> io::Result<ClientConfig> {
    let (certificate, secret_key) = generate_certificate()?;

    let server_cert_verifier = Arc::new(ServerCertificateVerifier {});

    let tls_config = rustls::ClientConfig::builder_with_protocol_versions(&[&TLS13])
        .dangerous()
        .with_custom_certificate_verifier(server_cert_verifier)
        .with_client_auth_cert(vec![certificate], secret_key)
        .map_err(|e| io::Error::other(format!("Failed to create QUIC client TLS config: {e}")))?;

    let quic_config: QuicClientConfig = tls_config
        .try_into()
        .map_err(|e| io::Error::other(format!("Failed to create QUIC client config: {e}")))?;

    let mut config = ClientConfig::new(Arc::new(quic_config));

    // Configure transport parameters
    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(15)));
    transport.max_idle_timeout(Some(VarInt::from_u32(30_000).into()));
    config.transport_config(Arc::new(transport));

    Ok(config)
}

/// Create QUIC server configuration with our TLS config
fn create_server_config() -> io::Result<ServerConfig> {
    let (certificate, secret_key) = generate_certificate()?;

    let client_cert_verifier = Arc::new(ClientCertificateVerifier {});

    let tls_config = rustls::ServerConfig::builder_with_protocol_versions(&[&TLS13])
        .with_client_cert_verifier(client_cert_verifier)
        .with_single_cert(vec![certificate], secret_key)
        .map_err(|e| io::Error::other(format!("Failed to create QUIC server TLS config: {e}")))?;

    let quic_config: QuicServerConfig = tls_config
        .try_into()
        .map_err(|e| io::Error::other(format!("Failed to create QUIC server config: {e}")))?;

    let mut config = ServerConfig::with_crypto(Arc::new(quic_config));

    // Configure transport parameters
    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(15)));
    transport.max_idle_timeout(Some(VarInt::from_u32(30_000).into()));
    config.transport_config(Arc::new(transport));

    Ok(config)
}

/// Wrapper around quinn's bidirectional stream to implement PtStream
pub struct QuicStream {
    send: SendStream,
    recv: RecvStream,
}

impl QuicStream {
    fn new(send: SendStream, recv: RecvStream) -> Self {
        Self { send, recv }
    }
}

impl AsyncRead for QuicStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.recv)
            .poll_read(cx, buf)
            .map_err(|e| io::Error::other(format!("QUIC read error: {e}")))
    }
}

impl AsyncWrite for QuicStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.send)
            .poll_write(cx, buf)
            .map_err(|e| io::Error::other(format!("QUIC write error: {e}")))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send)
            .poll_flush(cx)
            .map_err(|e| io::Error::other(format!("QUIC flush error: {e}")))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send)
            .poll_close(cx)
            .map_err(|e| io::Error::other(format!("QUIC close error: {e}")))
    }
}

/// QUIC Dialer implementation
#[derive(Clone, Debug)]
pub struct QuicDialer {
    endpoint: Endpoint,
}

impl QuicDialer {
    /// Instantiate a new [`QuicDialer`] object
    pub(crate) async fn new() -> io::Result<Self> {
        let client_config = create_client_config()?;

        // Bind to any available port for outgoing connections
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| io::Error::other(format!("Failed to create QUIC endpoint: {e}")))?;

        endpoint.set_default_client_config(client_config);

        Ok(Self { endpoint })
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        socket_addr: SocketAddr,
        timeout: Option<Duration>,
    ) -> io::Result<QuicStream> {
        debug!(target: "net::quic::do_dial", "Dialing {socket_addr} with QUIC...");

        let connect = async {
            // Connect to the remote endpoint
            let connection = self
                .endpoint
                .connect(socket_addr, TLS_DNS_NAME)
                .map_err(|e| io::Error::other(format!("QUIC connect error: {e}")))?
                .await
                .map_err(|e| io::Error::other(format!("QUIC connection error: {e}")))?;

            // Open a bidirectional stream
            let (send, recv) = connection
                .open_bi()
                .await
                .map_err(|e| io::Error::other(format!("QUIC stream error: {e}")))?;

            Ok(QuicStream::new(send, recv))
        };

        match timeout {
            Some(t) => {
                let timer = Timer::after(t);
                pin_mut!(timer);
                pin_mut!(connect);

                match select(connect, timer).await {
                    Either::Left((Ok(stream), _)) => Ok(stream),
                    Either::Left((Err(e), _)) => Err(e),
                    Either::Right((_, _)) => Err(io::ErrorKind::TimedOut.into()),
                }
            }
            None => connect.await,
        }
    }
}

/// QUIC Listener implementation
#[derive(Debug, Clone)]
pub struct QuicListener {
    /// When the user puts a port of 0, the OS will assign a random port.
    /// We get it from the listener so we know what the true endpoint is.
    pub port: Arc<OnceCell<u16>>,
}

impl QuicListener {
    /// Instantiate a new [`QuicListener`]
    pub async fn new() -> io::Result<Self> {
        Ok(Self { port: Arc::new(OnceCell::new()) })
    }

    /// Internal listen function
    pub(crate) async fn do_listen(
        &self,
        socket_addr: SocketAddr,
    ) -> io::Result<QuicListenerIntern> {
        let server_config = create_server_config()?;

        let endpoint = Endpoint::server(server_config, socket_addr)
            .map_err(|e| io::Error::other(format!("Failed to create QUIC server endpoint: {e}")))?;

        let local_port = endpoint.local_addr()?.port();

        debug!(
            target: "net::quic::do_listen",
            "Listening on QUIC endpoint: {}",
            endpoint.local_addr()?,
        );

        self.port.set(local_port).await.expect("fatal port already set for QuicListener");

        Ok(QuicListenerIntern { endpoint })
    }
}

/// Internal QUIC Listener implementation, used with `PtListener`
pub struct QuicListenerIntern {
    endpoint: Endpoint,
}

#[async_trait]
impl PtListener for QuicListenerIntern {
    async fn next(&self) -> io::Result<(Box<dyn PtStream>, Url)> {
        // Wait for an incoming connection
        let incoming =
            self.endpoint.accept().await.ok_or_else(|| {
                io::Error::new(io::ErrorKind::ConnectionAborted, "Endpoint closed")
            })?;

        let peer_addr = incoming.remote_address();

        let connection =
            incoming.await.map_err(|e| io::Error::other(format!("QUIC accept error: {e}")))?;

        // Accept a bidirectional stream from the client
        let (send, recv) = connection
            .accept_bi()
            .await
            .map_err(|e| io::Error::other(format!("QUIC stream accept error: {e}")))?;

        let url = Url::parse(&format!("quic://{peer_addr}")).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid peer address: {e}"))
        })?;

        Ok((Box::new(QuicStream::new(send, recv)), url))
    }
}
