use std::{io, net::SocketAddr, pin::Pin, sync::Arc, time::SystemTime};

use async_std::net::TcpStream;
use futures::prelude::*;
use futures_rustls::{
    rustls,
    rustls::{
        client::{ServerCertVerified, ServerCertVerifier},
        kx_group::X25519,
        version::TLS13,
        Certificate, ClientConfig, ServerName,
    },
    TlsConnector, TlsStream,
};
use log::debug;
use socket2::{Domain, Socket, Type};
use url::Url;

use super::{Transport, TransportError};

const CIPHER_SUITE: &str = "TLS13_CHACHA20_POLY1305_SHA256";

fn cipher_suite() -> rustls::SupportedCipherSuite {
    for suite in rustls::ALL_CIPHER_SUITES {
        let sname = format!("{:?}", suite.suite()).to_lowercase();

        if sname == CIPHER_SUITE.to_string().to_lowercase() {
            return *suite
        }
    }

    unreachable!()
}

struct ServerCertificateVerifier;

impl ServerCertVerifier for ServerCertificateVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // TODO: upsycle
        Ok(ServerCertVerified::assertion())
    }
}

pub struct TlsTransport {
    pub ttl: Option<u32>,
}

impl Transport for TlsTransport {
    type Output = TlsStream<TcpStream>;
    type Error = io::Error;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>> {
        if url.scheme() != "tls" {
            return Err(TransportError::AddrNotSupported(url))
        }

        debug!(target: "tlstransport", "dialing {}", url);
        Ok(Box::pin(self.do_dial(url)))
    }
}

impl TlsTransport {
    fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        if let Some(ttl) = self.ttl {
            socket.set_ttl(ttl)?;
        }

        Ok(socket)
    }

    async fn do_dial(self, url: Url) -> Result<TlsStream<TcpStream>, io::Error> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        // TODO: Handle host
        let server_name = ServerName::try_from("example.com").unwrap();

        let socket = self.create_socket(socket_addr)?;
        socket.set_nonblocking(true)?;

        // TODO: This should be in the struct
        // TODO: Client auth (see upsycle)
        let server_cert_verifier = Arc::new(ServerCertificateVerifier {});
        let config = ClientConfig::builder()
            .with_cipher_suites(&[cipher_suite()])
            .with_kx_groups(&[&X25519])
            .with_protocol_versions(&[&TLS13])
            .unwrap()
            .with_custom_certificate_verifier(server_cert_verifier)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));

        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err),
        };

        let stream = TcpStream::from(std::net::TcpStream::from(socket));
        let stream = connector.connect(server_name, stream).await?;
        Ok(TlsStream::Client(stream))
    }
}
