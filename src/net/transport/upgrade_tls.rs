/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::time::SystemTime;

use async_std::{net::TcpListener, sync::Arc};
use futures::prelude::*;
use futures_rustls::{
    rustls,
    rustls::{
        client::{ServerCertVerified, ServerCertVerifier},
        kx_group::X25519,
        server::{ClientCertVerified, ClientCertVerifier},
        version::TLS13,
        Certificate, ClientConfig, DistinguishedName, ServerConfig, ServerName,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};
use rustls_pemfile::pkcs8_private_keys;

use crate::Result;

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
        _scrs: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        // TODO: upsycle
        Ok(ServerCertVerified::assertion())
    }
}

struct ClientCertificateVerifier;
impl ClientCertVerifier for ClientCertificateVerifier {
    fn client_auth_root_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _now: SystemTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        // TODO: upsycle
        Ok(ClientCertVerified::assertion())
    }
}

pub struct TlsUpgrade {
    /// TLS server configuration
    server_config: Arc<ServerConfig>,
    /// TLS client configuration
    client_config: Arc<ClientConfig>,
}

impl TlsUpgrade {
    pub fn new() -> Self {
        // On each instantiation, generate a new keypair and certificate.
        let keypair_pem = ed25519_compact::KeyPair::generate().to_pem();
        let secret_key = pkcs8_private_keys(&mut keypair_pem.as_bytes()).unwrap();
        let secret_key = rustls::PrivateKey(secret_key[0].clone());

        let altnames = vec![String::from("dark.fi")];
        let mut cert_params = rcgen::CertificateParams::new(altnames);
        cert_params.alg = &rcgen::PKCS_ED25519;
        cert_params.key_pair = Some(rcgen::KeyPair::from_pem(&keypair_pem).unwrap());

        let certificate = rcgen::Certificate::from_params(cert_params).unwrap();
        let certificate = rustls::Certificate(certificate.serialize_der().unwrap());

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

        Self { server_config, client_config }
    }

    pub async fn upgrade_listener_tls(
        self,
        listener: TcpListener,
    ) -> Result<(TlsAcceptor, TcpListener)> {
        Ok((TlsAcceptor::from(self.server_config), listener))
    }

    pub async fn upgrade_dialer_tls<IO>(self, stream: IO) -> Result<TlsStream<IO>>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let server_name = ServerName::try_from("dark.fi").unwrap();
        let connector = TlsConnector::from(self.client_config);
        let stream = connector.connect(server_name, stream).await?;
        Ok(TlsStream::Client(stream))
    }
}

impl Default for TlsUpgrade {
    fn default() -> Self {
        Self::new()
    }
}
