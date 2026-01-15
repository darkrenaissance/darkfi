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
    collections::HashMap,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, OnceLock},
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
    lock::{Mutex, OnceCell},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EndpointKey {
    is_ipv6: bool,
    port: u16,
}

impl EndpointKey {
    fn from_addr(addr: SocketAddr) -> Self {
        Self { is_ipv6: addr.is_ipv6(), port: addr.port() }
    }
}

/// Global registry of QUIC endpoints, keyed by (addr_family, port).
/// This enables transparent endpoint sharing between Dialer and Listener.
static ENDPOINT_REGISTRY: OnceLock<Mutex<EndpointRegistry>> = OnceLock::new();

struct EndpointRegistry {
    endpoints: HashMap<EndpointKey, Endpoint>,
}

impl EndpointRegistry {
    fn new() -> Self {
        Self { endpoints: HashMap::new() }
    }

    /// Find an endpoint suitable for dialing the given target address.
    fn find_for_target(&self, target: SocketAddr) -> Option<Endpoint> {
        let is_ipv6 = target.is_ipv6();
        self.endpoints.iter().find(|(k, _)| k.is_ipv6 == is_ipv6).map(|(_, ep)| ep.clone())
    }
}

fn registry() -> &'static Mutex<EndpointRegistry> {
    ENDPOINT_REGISTRY.get_or_init(|| Mutex::new(EndpointRegistry::new()))
}

/// Register an endpoint for the given bind address.
/// Returns the endpoint (may be existing if already registered).
async fn register_endpoint(bind_addr: SocketAddr) -> io::Result<Endpoint> {
    let mut reg = registry().lock().await;

    let key = EndpointKey::from_addr(bind_addr);

    // Check if we already have an endpoint for this (family, port)
    if bind_addr.port() != 0 {
        if let Some(endpoint) = reg.endpoints.get(&key) {
            debug!(
                target: "net::quic::registry",
                "[QUIC] Reusing existing {} endpoint on port {}",
                if key.is_ipv6 { "IPv6" } else { "IPv4" },
                key.port,
            );
            return Ok(endpoint.clone())
        }
    }

    // Create new dual-mode endpoint
    let endpoint = create_dual_endpoint(bind_addr).await?;
    let actual_port = endpoint.local_addr()?.port();

    let actual_key = EndpointKey { is_ipv6: key.is_ipv6, port: actual_port };

    debug!(
        target: "net::quic::registry",
        "[QUIC] Created new {} QUIC endpoint on port {}",
        if actual_key.is_ipv6 { "IPv6" } else { "IPv4" },
        actual_port,
    );

    reg.endpoints.insert(actual_key, endpoint.clone());

    Ok(endpoint)
}

/// Get an endpoint suitable for dialing the given target address.
/// If no matching endpoint exist, creates a new one.
async fn get_endpoint_for_target(target: SocketAddr) -> io::Result<Endpoint> {
    let reg = registry().lock().await;
    if let Some(endpoint) = reg.find_for_target(target) {
        debug!(
            target: "net::quic::registry",
            "[QUIC] Dialer using existing {} endpoint on port {}",
            if target.is_ipv6() { "IPv6" } else { "IPv4" },
            endpoint.local_addr().map(|a| a.port()).unwrap_or(0),
        );
        return Ok(endpoint)
    }
    drop(reg);

    // No suitable endpoint, create one.
    let bind_addr: SocketAddr =
        if target.is_ipv6() { "[::]:0".parse().unwrap() } else { "0.0.0.0:0".parse().unwrap() };

    debug!(
        target: "net::quic::registry",
        "[QUIC] Creating new {} endpoint for dialing",
        if target.is_ipv6() { "IPv6" } else { "IPv4" },
    );

    register_endpoint(bind_addr).await
}

/// Create an endpoint configured for both client and server roles
async fn create_dual_endpoint(bind_addr: SocketAddr) -> io::Result<Endpoint> {
    let server_config = create_server_config()?;
    let client_config = create_client_config()?;

    let endpoint = Endpoint::server(server_config, bind_addr)
        .map_err(|e| io::Error::other(format!("Failed to create QUIC endpoint: {e}")))?;

    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}

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

/// QUIC Dialer implementation.
///
/// Automatically shares endpoint with QuicListener when one exists,
/// enabling NAT hole punching without any special configuration.
#[derive(Clone, Debug)]
pub struct QuicDialer;

impl QuicDialer {
    /// Instantiate a new [`QuicDialer`] object
    ///
    /// The actual endpoint is selected at dial-time based on the target.
    pub(crate) async fn new() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        socket_addr: SocketAddr,
        timeout: Option<Duration>,
    ) -> io::Result<QuicStream> {
        // Get appropriate endpoint for target address family
        let endpoint = get_endpoint_for_target(socket_addr).await?;

        debug!(
            target: "net::quic::do_dial",
            "[QUIC] Dialing {} {} from local {}",
            if socket_addr.is_ipv6() { "IPv6" } else { "IPv4" },
            socket_addr,
            endpoint.local_addr().map(|a| a.to_string()).unwrap_or_default(),
        );

        let connect = async {
            // Connect to the remote endpoint
            let connection = endpoint
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
///
/// When created, registers its endpoint so that QuicDialer can share it,
/// enabling NAT hole punching automatically.
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
        let endpoint = register_endpoint(socket_addr).await?;

        let local_addr = endpoint.local_addr()?;

        debug!(
            target: "net::quic::do_listen",
            "[QUIC] Listening on {} QUIC endpoint: {}",
            if local_addr.is_ipv6() { "IPv6" } else { "IPv4" },
            local_addr,
        );

        self.port.set(local_addr.port()).await.expect("fatal port already set for QuicListener");

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
