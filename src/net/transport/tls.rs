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

use async_rustls::{
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
use async_std::sync::Arc;
use log::error;
use rustls_pemfile::pkcs8_private_keys;
use x509_parser::{
    extensions::{GeneralName, ParsedExtension},
    parse_x509_certificate,
    prelude::{FromDer, X509Certificate},
    x509::SubjectPublicKeyInfo,
};

use crate::{util::encoding::base32, Result};

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

/// Validate that the altName pubkey is the same as the certificate's pubkey.
/// Returns `ed25519_compact::PublicKey` on success.
fn validate_pubkey(
    cert: &X509Certificate,
) -> std::result::Result<ed25519_compact::PublicKey, rustls::Error> {
    // We keep a public key in the altName, so we need to grab it.
    // We compare that the actual public key of the certificate is
    // the same as that one, and then we return it.
    // The actual verification functions handle signature verification.
    #[rustfmt::skip]
    let oid = x509_parser::oid_registry::asn1_rs::oid!(2.5.29.17);
    let Ok(Some(extension)) = cert.get_extension_unique(&oid) else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    // Crufty AF
    // (ノಠ益ಠ)ノ彡┻━┻
    let pubkey_bytes = match extension.parsed_extension() {
        ParsedExtension::SubjectAlternativeName(altname) => {
            if altname.general_names.len() != 1 {
                return Err(rustls::CertificateError::BadEncoding.into())
            }

            match altname.general_names[0] {
                GeneralName::DNSName(a) => base32::decode(a),
                _ => return Err(rustls::CertificateError::BadEncoding.into()),
            }
        }
        _ => return Err(rustls::CertificateError::BadEncoding.into()),
    };

    let Some(pubkey_bytes) = pubkey_bytes else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    if pubkey_bytes.len() != 32 {
        return Err(rustls::CertificateError::BadEncoding.into())
    }

    let pubkey = ed25519_compact::PublicKey::new(pubkey_bytes.try_into().unwrap());
    let pubkey_der = pubkey.to_der();

    let Ok((_, parsed_pubkey)) = SubjectPublicKeyInfo::from_der(&pubkey_der) else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    let Ok(parsed_name_pubkey) = parsed_pubkey.parsed() else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    let Ok(parsed_cert_pubkey) = cert.public_key().parsed() else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    if parsed_name_pubkey != parsed_cert_pubkey {
        return Err(rustls::CertificateError::BadSignature.into())
    }

    Ok(pubkey)
}

struct ServerCertificateVerifier;
impl ServerCertVerifier for ServerCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &ServerName,
        _scrs: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        // Parse the actual end_entity certificate
        let Ok((_, cert)) = parse_x509_certificate(&end_entity.0) else {
            error!(target: "net::tls", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Validate that the pubkey in altNames matches the certificate pubkey.
        if let Err(e) = validate_pubkey(&cert) {
            error!(target: "net::tls", "[net::tls] Failed verifying server certificate signature: {}", e);
            return Err(e)
        }

        // Verify the signature. By passing `None` it should use the certificate
        // pubkey, but we also verified that it matches the one in altNames above.
        if let Err(e) = cert.verify_signature(None) {
            error!(target: "net::tls", "[net::tls] Failed verifying server certificate signature: {}", e);
            return Err(rustls::CertificateError::BadSignature.into())
        }

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
        end_entity: &Certificate,
        _intermediates: &[Certificate],
        _now: SystemTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        // Parse the actual end_entity certificate
        let Ok((_, cert)) = parse_x509_certificate(&end_entity.0) else {
            error!(target: "net::tls", "[net::tls] Failed parsing client TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Validate that the pubkey in altNames matches the certificate pubkey.
        if let Err(e) = validate_pubkey(&cert) {
            error!(target: "net::tls", "[net::tls] Failed verifying client certificate signature: {}", e);
            return Err(e)
        }

        // Verify the signature. By passing `None` it should use the certificate
        // pubkey, but we also verified that it matches the one in altNames above.
        if let Err(e) = cert.verify_signature(None) {
            error!(target: "net::tls", "[net::tls] Failed verifying client certificate signature: {}", e);
            return Err(rustls::CertificateError::BadSignature.into())
        }

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
        let keypair = ed25519_compact::KeyPair::generate();
        let keypair_pem = keypair.to_pem();
        let secret_key = pkcs8_private_keys(&mut keypair_pem.as_bytes()).unwrap();
        let secret_key = rustls::PrivateKey(secret_key[0].clone());

        let altnames = vec![base32::encode(false, keypair.pk.as_slice())];

        let mut cert_params = rcgen::CertificateParams::new(altnames);
        cert_params.alg = &rcgen::PKCS_ED25519;
        cert_params.key_pair = Some(rcgen::KeyPair::from_pem(&keypair_pem).unwrap());
        cert_params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        ];

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

        Self { server_config, client_config }
    }

    pub async fn upgrade_dialer_tls<IO>(self, stream: IO) -> Result<TlsStream<IO>>
    where
        IO: super::PtStream,
    {
        let server_name = ServerName::try_from("dark.fi").unwrap();
        let connector = TlsConnector::from(self.client_config);
        let stream = connector.connect(server_name, stream).await?;
        Ok(TlsStream::Client(stream))
    }

    // FIXME: Try to find a transparent way for this instead of implementing separately for all
    #[cfg(feature = "p2p-transport-tcp")]
    pub async fn upgrade_listener_tcp_tls(
        self,
        listener: async_std::net::TcpListener,
    ) -> Result<(TlsAcceptor, async_std::net::TcpListener)> {
        Ok((TlsAcceptor::from(self.server_config), listener))
    }
}

impl Default for TlsUpgrade {
    fn default() -> Self {
        Self::new()
    }
}
