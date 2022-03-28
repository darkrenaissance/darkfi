use std::{io, net::SocketAddr, pin::Pin, sync::Arc, time::SystemTime};

use async_std::net::{TcpListener, TcpStream};
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
    pub ttl: Option<u32>,
}

impl Transport for TlsTransport {
    type Acceptor = (TlsAcceptor, TcpListener);
    type Connector = TlsStream<TcpStream>;

    type Error = io::Error;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor, Self::Error>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector, Self::Error>> + Send>>;

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

    async fn do_listen(self, url: Url) -> Result<(TlsAcceptor, TcpListener), io::Error> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        // TODO: make backlog configurable
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;

        // TODO: This should be in the struct
        // TODO: Client auth (see upsycle)
        let keypair_pem = ed25519_compact::KeyPair::generate().to_pem();
        let secret_key = pkcs8_private_keys(&mut keypair_pem.as_bytes())?;
        let secret_key = rustls::PrivateKey(secret_key[0].clone());

        // TODO: Into util
        let altnames = vec![String::from("example.com")];
        let mut cert_params = rcgen::CertificateParams::new(altnames);
        cert_params.alg = &rcgen::PKCS_ED25519;
        cert_params.key_pair = Some(rcgen::KeyPair::from_pem(&keypair_pem).unwrap());

        let certificate = rcgen::Certificate::from_params(cert_params).unwrap();
        let cert_der = certificate.serialize_der().unwrap();
        let certificate = rustls::Certificate(cert_der);

        let _client_cert_verifier = Arc::new(ClientCertificateVerifier {});
        let config = ServerConfig::builder()
            .with_cipher_suites(&[cipher_suite()])
            .with_kx_groups(&[&X25519])
            .with_protocol_versions(&[&TLS13])
            .unwrap()
            // TODO: .with_client_cert_verifier(client_cert_verifier)
            .with_no_client_auth()
            .with_single_cert(vec![certificate], secret_key)
            .unwrap();

        let listener = TcpListener::from(std::net::TcpListener::from(socket));
        let acceptor = TlsAcceptor::from(Arc::new(config));
        Ok((acceptor, listener))
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
