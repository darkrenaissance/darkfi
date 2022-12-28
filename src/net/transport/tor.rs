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

use std::{
    io,
    io::{BufRead, BufReader, Write},
    net::SocketAddr,
    pin::Pin,
    time::Duration,
};

use async_std::{
    net::{TcpListener, TcpStream},
    sync::Arc,
};
use fast_socks5::client::{Config, Socks5Stream};
use futures::prelude::*;
use futures_rustls::{TlsAcceptor, TlsStream};
use socket2::{Domain, Socket, TcpKeepalive, Type};
use url::Url;

use crate::{Error, Result};

use super::{TlsUpgrade, Transport, TransportStream};

/// Implements communication through the tor proxy service.
///
/// ## Dialing
///
/// The tor service must be running for dialing to work. Url of it has to be passed to the
/// constructor.
///
/// ## Listening
///
/// Two ways of setting up hidden services are allowed: hidden services manually set up by the user
/// in the torc file or ephemereal hidden services created and deleted on the fly. For the latter,
/// the user must set up the tor control port[^controlport].
///
/// Having manually configured services forces the program to use pre-defined ports, i.e. it has no
/// way of changing them.
///
/// Before calling [listen_on][transportlisten] on a local address, make sure that either a hidden
/// service pointing to that address was configured or that [create_ehs][torcreateehs] was called
/// with this address.
///
/// [^controlport] [Open control port](https://wiki.archlinux.org/title/tor#Open_Tor_ControlPort)
///
/// ### Warning on cloning
/// Cloning this structure increments the reference count to the already open
/// socket, which means ephemereal hidden services opened with the cloned instance will live as
/// long as there are clones. For this reason, I'd clone it only when you are sure you want this
/// behaviour. Don't be lazy!
///
/// [transportlisten]: Transport
/// [torcreateehs]: TorTransport::create_ehs
#[derive(Clone)]
pub struct TorTransport {
    socks_url: Url,
    tor_controller: Option<TorController>,
}

/// Represents information needed to communicate with the Tor control socket
#[derive(Clone)]
struct TorController {
    socket: Arc<Socket>, // Need to hold this socket open as long as the tor trasport is alive, so ephemeral services are dropped when TorTransport is dropped
    auth: String,
}

/// Contains the configuration to communicate with the Tor Controler
///
/// When cloned, the socket is not reopened since we use reference count.
/// The hidden services created live as long as clones of the struct.
impl TorController {
    /// Creates a new TorTransport
    ///
    /// # Arguments
    ///
    /// * `url` - url to connect to the tor control. For example tcp://127.0.0.1:9051
    ///
    /// * `auth` - either authentication cookie bytes (32 bytes) as hex in a string
    /// or a password as a quoted string.
    ///
    /// Cookie string: `assert_eq!(auth,"886b9177aec471965abd34b6a846dc32cf617dcff0625cba7a414e31dd4b75a0")`
    ///
    /// Password string: `assert_eq!(auth,"\"mypassword\"")`
    pub fn new(url: Url, auth: String) -> Result<Self> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;
        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err.into()),
        };
        Ok(Self { socket: Arc::new(socket), auth })
    }
    /// Creates an ephemeral hidden service pointing to local address, returns onion address
    ///
    /// # Arguments
    ///
    /// * `url` - url that the hidden service maps to.
    pub fn create_ehs(&self, url: Url) -> Result<Url> {
        let local_socket = self.socket.try_clone()?;
        let mut stream = std::net::TcpStream::from(local_socket);

        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let host = url.host().unwrap();
        let port = url.port().unwrap();

        let payload = format!(
            "AUTHENTICATE {a}\r\nADD_ONION NEW:ED25519-V3 Flags=DiscardPK Port={p},{h}:{p}\r\n",
            a = self.auth,
            p = port,
            h = host
        );
        stream.write_all(payload.as_bytes())?;
        // 1s is maybe a bit too much. Gives tor time to reply
        stream.set_read_timeout(Some(Duration::from_secs(1)))?;
        let mut reader = BufReader::new(stream);
        let mut repl = String::new();
        while let Ok(nbytes) = reader.read_line(&mut repl) {
            if nbytes == 0 {
                break
            }
        }

        let spl: Vec<&str> = repl.split('\n').collect();
        if spl.len() != 4 {
            return Err(Error::TorError(format!("Unsuccessful reply from TorControl: {:?}", spl)))
        }

        let onion: Vec<&str> = spl[1].split('=').collect();
        if onion.len() != 2 {
            return Err(Error::TorError(format!("Unsuccessful reply from TorControl: {:?}", spl)))
        }

        let onion = &onion[1][..onion[1].len() - 1];
        let hurl = format!("tcp://{}.onion:{}", onion, port);
        Ok(Url::parse(&hurl)?)
    }
}

impl TorTransport {
    /// Creates a new TorTransport
    ///
    /// # Arguments
    ///
    /// * `socks_url` - url to connect to the tor service. For example socks5://127.0.0.1:9050
    ///
    /// * `control_info` - Possibility to open a control connection to create ephemeral hidden
    /// services that live as long as the TorTransport.
    /// It is a tuple of the control socket url and authentication cookie as string
    /// represented in hex.
    pub fn new(socks_url: Url, control_info: Option<(Url, String)>) -> Result<Self> {
        match control_info {
            Some(info) => {
                let (url, auth) = info;
                let tor_controller = Some(TorController::new(url, auth)?);
                Ok(Self { socks_url, tor_controller })
            }
            None => Ok(Self { socks_url, tor_controller: None }),
        }
    }

    /// Query the environment for listener Tor variables, or fallback to defaults
    pub fn get_listener_env() -> Result<(Url, Url, String)> {
        let socks5_url = Url::parse(
            &std::env::var("DARKFI_TOR_SOCKS5_URL")
                .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
        )?;

        let torc_url = Url::parse(
            &std::env::var("DARKFI_TOR_CONTROL_URL")
                .unwrap_or_else(|_| "tcp://127.0.0.1:9051".to_string()),
        )?;

        let auth_cookie = std::env::var("DARKFI_TOR_COOKIE");
        if auth_cookie.is_err() {
            return Err(Error::TorError(
                "Please set the env var DARKFI_TOR_COOKIE to the configured Tor cookie file.\n\
                For example:\n\
                export DARKFI_TOR_COOKIE='/var/lib/tor/control_auth_cookie'"
                    .to_string(),
            ))
        }

        Ok((socks5_url, torc_url, auth_cookie.unwrap()))
    }

    /// Query the environment for the dialer Tor variables, or fallback to defaults
    pub fn get_dialer_env() -> Result<Url> {
        Ok(Url::parse(
            &std::env::var("DARKFI_TOR_SOCKS5_URL")
                .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
        )?)
    }

    /// Creates an ephemeral hidden service pointing to local address, returns onion address
    /// when successful.
    ///
    /// # Arguments
    ///
    /// * `url` - url that the hidden service maps to.
    pub fn create_ehs(&self, url: Url) -> Result<Url> {
        let tor_controller = self.tor_controller.as_ref();

        if tor_controller.is_none() {
            return Err(Error::TorError("No controller configured for this transport".to_string()))
        };

        tor_controller.unwrap().create_ehs(url)
    }

    pub async fn do_dial(self, url: Url) -> Result<Socks5Stream<TcpStream>> {
        let socks_url_str = self.socks_url.socket_addrs(|| None)?[0].to_string();
        let host = url.host().unwrap().to_string();
        let port = url.port().unwrap_or(80);
        let config = Config::default();
        let stream = if !self.socks_url.username().is_empty() && self.socks_url.password().is_some()
        {
            Socks5Stream::connect_with_password(
                socks_url_str,
                host,
                port,
                self.socks_url.username().to_string(),
                self.socks_url.password().unwrap().to_string(),
                config,
            )
            .await?
        } else {
            Socks5Stream::connect(socks_url_str, host, port, config).await?
        };
        Ok(stream)
    }

    fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        // TODO: Perhaps make these configurable
        socket.set_nodelay(true)?;
        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(30));
        socket.set_tcp_keepalive(&keepalive)?;
        // TODO: Make sure to disallow running multiple instances of a program using this.
        socket.set_reuse_port(true)?;

        Ok(socket)
    }

    pub async fn do_listen(self, url: Url) -> Result<TcpListener> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }
}

impl<T: TransportStream> TransportStream for Socks5Stream<T> {}

impl Transport for TorTransport {
    type Acceptor = TcpListener;
    type Connector = Socks5Stream<TcpStream>;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector>> + Send>>;

    type TlsListener = Pin<Box<dyn Future<Output = Result<(TlsAcceptor, Self::Acceptor)>> + Send>>;
    type TlsDialer = Pin<Box<dyn Future<Output = Result<TlsStream<Self::Connector>>> + Send>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener> {
        match url.scheme() {
            "tor" | "tor+tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }
        Ok(Box::pin(self.do_listen(url)))
    }

    fn upgrade_listener(self, acceptor: Self::Acceptor) -> Result<Self::TlsListener> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_listener_tls(acceptor)))
    }

    fn dial(self, url: Url, _timeout: Option<Duration>) -> Result<Self::Dial> {
        match url.scheme() {
            "tor" | "tor+tls" => {}
            "tcp" | "tcp+tls" | "tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }
        Ok(Box::pin(self.do_dial(url)))
    }

    fn upgrade_dialer(self, connector: Self::Connector) -> Result<Self::TlsDialer> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_dialer_tls(connector)))
    }
}
