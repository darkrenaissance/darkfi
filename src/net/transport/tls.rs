/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::sync::Arc;

use futures_rustls::{
    rustls::{
        self,
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
        server::danger::{ClientCertVerified, ClientCertVerifier},
        version::TLS13,
        ClientConfig, DigitallySignedStruct, DistinguishedName, ServerConfig, SignatureScheme,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};
use log::error;
use rustls_pemfile::pkcs8_private_keys;
use x509_parser::{
    parse_x509_certificate,
    prelude::{GeneralName, ParsedExtension, X509Certificate},
};

use crate::Result;

/// Validate certificate DNSName.
fn validate_dnsname(cert: &X509Certificate) -> std::result::Result<(), rustls::Error> {
    #[rustfmt::skip]
        let oid = x509_parser::oid_registry::asn1_rs::oid!(2.5.29.17);
    let Ok(Some(extension)) = cert.get_extension_unique(&oid) else {
        return Err(rustls::CertificateError::BadEncoding.into())
    };

    let dns_name = match extension.parsed_extension() {
        ParsedExtension::SubjectAlternativeName(altname) => {
            if altname.general_names.len() != 1 {
                return Err(rustls::CertificateError::BadEncoding.into())
            }

            match altname.general_names[0] {
                GeneralName::DNSName(dns_name) => dns_name,
                _ => return Err(rustls::CertificateError::BadEncoding.into()),
            }
        }

        _ => return Err(rustls::CertificateError::BadEncoding.into()),
    };

    if dns_name != "dark.fi" {
        return Err(rustls::CertificateError::BadEncoding.into())
    }

    Ok(())
}

#[derive(Debug)]
struct ServerCertificateVerifier;
impl ServerCertVerifier for ServerCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        // Read the DER-encoded certificate into a buffer
        let mut buf = Vec::with_capacity(end_entity.len());
        for byte in end_entity.iter() {
            buf.push(*byte);
        }

        // Parse the certificate
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            error!(target: "net::tls::verify_server_cert", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Validate DNSName
        validate_dnsname(&cert)?;

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        unreachable!()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        // Verify we're using the correct signature scheme
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::CertificateError::BadSignature.into())
        }

        // Read the DER-encoded certificate into a buffer
        let mut buf = Vec::with_capacity(cert.len());
        for byte in cert.iter() {
            buf.push(*byte);
        }

        // Parse the certificate and extract the public key
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        let Ok(public_key) = ed25519_compact::PublicKey::from_der(cert.public_key().raw) else {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed parsing server public key");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Verify the signature
        let Ok(signature) = ed25519_compact::Signature::from_slice(dss.signature()) else {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed verifying server signature");
            return Err(rustls::CertificateError::BadSignature.into())
        };

        if let Err(e) = public_key.verify(message, &signature) {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed verifying server signature: {}", e);
            return Err(rustls::CertificateError::BadSignature.into())
        }

        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

#[derive(Debug)]
struct ClientCertificateVerifier;
impl ClientCertVerifier for ClientCertificateVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        // Read the DER-encoded certificate into a buffer
        let mut cert = Vec::with_capacity(end_entity.len());
        for byte in end_entity.iter() {
            cert.push(*byte);
        }

        // Parse the certificate
        let Ok((_, cert)) = parse_x509_certificate(&cert) else {
            error!(target: "net::tls::verify_server_cert", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Validate DNSName
        validate_dnsname(&cert)?;

        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        unreachable!()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        // Verify we're using the correct signature scheme
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::CertificateError::BadSignature.into())
        }

        // Read the DER-encoded certificate into a buffer
        let mut buf = Vec::with_capacity(cert.len());
        for byte in cert.iter() {
            buf.push(*byte);
        }

        // Parse the certificate and extract the public key
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed parsing server TLS certificate");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        let Ok(public_key) = ed25519_compact::PublicKey::from_der(cert.public_key().raw) else {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed parsing server public key");
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        // Verify the signature
        let Ok(signature) = ed25519_compact::Signature::from_slice(dss.signature()) else {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed verifying server signature");
            return Err(rustls::CertificateError::BadSignature.into())
        };

        if let Err(e) = public_key.verify(message, &signature) {
            error!(target: "net::tls::verify_tls13_signature", "[net::tls] Failed verifying server signature: {}", e);
            return Err(rustls::CertificateError::BadSignature.into())
        }

        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

pub struct TlsUpgrade {
    /// TLS server configuration
    server_config: Arc<ServerConfig>,
    /// TLS client configuration
    client_config: Arc<ClientConfig>,
}

impl TlsUpgrade {
    pub async fn new() -> Self {
        // On each instantiation, generate a new keypair and certificate
        let keypair_pem = ed25519_compact::KeyPair::generate().to_pem();
        let secret_key = pkcs8_private_keys(&mut keypair_pem.as_bytes()).next().unwrap().unwrap();
        let secret_key = PrivateKeyDer::Pkcs8(secret_key);

        let mut cert_params = rcgen::CertificateParams::new(&[]);
        cert_params.alg = &rcgen::PKCS_ED25519;
        cert_params.key_pair = Some(rcgen::KeyPair::from_pem(&keypair_pem).unwrap());
        cert_params.subject_alt_names = vec![rcgen::SanType::DnsName("dark.fi".to_string())];
        cert_params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        ];

        let certificate = rcgen::Certificate::from_params(cert_params).unwrap();
        let certificate = certificate.serialize_der().unwrap();

        // Server-side config
        let client_cert_verifier = Arc::new(ClientCertificateVerifier {});
        let server_config = Arc::new(
            ServerConfig::builder_with_protocol_versions(&[&TLS13])
                .with_client_cert_verifier(client_cert_verifier)
                .with_single_cert(vec![certificate.clone().into()], secret_key.clone_key())
                .unwrap(),
        );

        // Client-side config
        let server_cert_verifier = Arc::new(ServerCertificateVerifier {});
        let client_config = Arc::new(
            ClientConfig::builder_with_protocol_versions(&[&TLS13])
                .dangerous()
                .with_custom_certificate_verifier(server_cert_verifier)
                .with_client_auth_cert(vec![certificate.into()], secret_key)
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

    // TODO: Try to find a transparent way for this instead of implementing
    // the function separately for every transport type.
    #[cfg(feature = "p2p-tcp")]
    pub async fn upgrade_listener_tcp_tls(
        self,
        listener: smol::net::TcpListener,
    ) -> Result<(TlsAcceptor, smol::net::TcpListener)> {
        Ok((TlsAcceptor::from(self.server_config), listener))
    }
}
