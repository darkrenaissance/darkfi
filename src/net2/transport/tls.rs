use async_std::net::{TcpListener, TcpStream};
use std::{io, net::SocketAddr, pin::Pin, sync::Arc, time::SystemTime};

use async_trait::async_trait;
use futures::prelude::*;
use futures_rustls::{
    rustls,
    rustls::{
        client::{ServerCertVerified, ServerCertVerifier},
        kx_group::X25519,
        server::{ClientCertVerified, ClientCertVerifier},
        version::TLS13,
        Certificate, ClientConfig, DistinguishedNames, ServerConfig, ServerName,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};
use log::debug;
use rustls_pemfile::pkcs8_private_keys;
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

struct ClientCertificateVerifier;
impl ClientCertVerifier for ClientCertificateVerifier {
    fn client_auth_root_subjects(&self) -> Option<DistinguishedNames> {
        Some(vec![])
    }

    fn verify_client_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _now: SystemTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        // TODO: upsycle
        Ok(ClientCertVerified::assertion())
    }
}

#[derive(Clone)]
pub struct TlsTransport {
    /// TTL to set for opened sockets, or `None` for default
    ttl: Option<u32>,
    /// Size of the listen backlog for listen sockets
    backlog: i32,
    /// TLS server configuration
    server_config: Arc<ServerConfig>,
    /// TLS client configuration
    client_config: Arc<ClientConfig>,
}

#[async_trait]
impl Transport for TlsTransport {
    type Acceptor = (TlsAcceptor, TcpListener);
    type Connector = TlsStream<TcpStream>;

    type Error = io::Error;

    type Listener = Pin<
        Box<dyn Future<Output = Result<Self::Acceptor, TransportError<Self::Error>>> + Send + Sync>,
    >;
    type Dial = Pin<
        Box<
            dyn Future<Output = Result<Self::Connector, TransportError<Self::Error>>> + Send + Sync,
        >,
    >;

    fn listen_on(self, url: Url) -> Result<Self::Listener, TransportError<Self::Error>> {
        if url.scheme() != "tls" {
            return Err(TransportError::AddrNotSupported(url))
        }

        debug!(target: "tlstransport", "listening on {}", url);
        Ok(Box::pin(self.do_listen(url)))
    }

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>> {
        if url.scheme() != "tls" {
            return Err(TransportError::AddrNotSupported(url))
        }

        debug!(target: "tlstransport", "dialing {}", url);
        Ok(Box::pin(self.do_dial(url)))
    }

    fn new(ttl: Option<u32>, backlog: i32) -> Self {
        // On each instantiation, generate a new keypair and certificate
        let keypair_pem = ed25519_compact::KeyPair::generate().to_pem();
        let secret_key = pkcs8_private_keys(&mut keypair_pem.as_bytes()).unwrap();
        let secret_key = rustls::PrivateKey(secret_key[0].clone());

        let altnames = vec![String::from("dark.fi")];
        let mut cert_params = rcgen::CertificateParams::new(altnames);
        cert_params.alg = &rcgen::PKCS_ED25519;
        cert_params.key_pair = Some(rcgen::KeyPair::from_pem(&keypair_pem).unwrap());

        let certificate = rcgen::Certificate::from_params(cert_params).unwrap();
        let certificate = certificate.serialize_der().unwrap();
        let certificate = rustls::Certificate(certificate);

        let client_cert_verifier = Arc::new(ClientCertificateVerifier {});
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_cipher_suites(&[cipher_suite()])
                .with_kx_groups(&[&X25519])
                .with_protocol_versions(&[&TLS13])
                .unwrap()
                .with_client_cert_verifier(client_cert_verifier)
                .with_single_cert(vec![certificate.clone()], secret_key.clone())
                .unwrap(),
        );

        let server_cert_verifier = Arc::new(ServerCertificateVerifier {});
        let client_config = Arc::new(
            ClientConfig::builder()
                .with_cipher_suites(&[cipher_suite()])
                .with_kx_groups(&[&X25519])
                .with_protocol_versions(&[&TLS13])
                .unwrap()
                .with_custom_certificate_verifier(server_cert_verifier)
                .with_single_cert(vec![certificate], secret_key)
                .unwrap(),
        );

        Self { ttl, backlog, server_config, client_config }
    }

    async fn accept(
        listener: Arc<Self::Acceptor>,
    ) -> Result<Self::Connector, TransportError<Self::Error>> {
        let stream = listener.1.accept().await?.0;
        Ok(listener.0.accept(stream).await?.into())
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

    async fn do_listen(
        self,
        url: Url,
    ) -> Result<(TlsAcceptor, TcpListener), TransportError<io::Error>> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(self.backlog)?;
        socket.set_nonblocking(true)?;

        let listener = TcpListener::from(std::net::TcpListener::from(socket));
        let acceptor = TlsAcceptor::from(self.server_config);
        Ok((acceptor, listener))
    }

    async fn do_dial(self, url: Url) -> Result<TlsStream<TcpStream>, TransportError<io::Error>> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let server_name = ServerName::try_from("dark.fi").unwrap();
        let socket = self.create_socket(socket_addr)?;
        socket.set_nonblocking(true)?;

        let connector = TlsConnector::from(self.client_config);

        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(TransportError::Other(err)),
        };

        let stream = TcpStream::from(std::net::TcpStream::from(socket));
        let stream = connector.connect(server_name, stream).await?;
        Ok(TlsStream::Client(stream))
    }
}
