use super::{Transport, TransportError};
use async_std::{
    net::{TcpListener, TcpStream},
    sync::Arc,
};
use fast_socks5::{
    client::{Config, Socks5Stream},
    Result, SocksError,
};
use futures::prelude::*;

use regex::Regex;
use socket2::{Domain, Socket, Type};
use std::{
    io,
    io::{BufRead, BufReader, Write},
    net::SocketAddr,
    pin::Pin,
    time::Duration,
};
use url::Url;

/// Implements communication through the tor proxy service
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

/// Wraps the errors, because dialing and listening use different communication
#[derive(Debug, thiserror::Error)]
pub enum TorError {
    #[error("Transport IO Error: {0}")]
    IoError(#[from] io::Error),
    #[error("Socks: {0}")]
    Socks5Error(#[from] SocksError),
    #[error("Url parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Regex parse error: {0}")]
    RegexError(#[from] regex::Error),
    #[error("Unexpected response from tor: {0}")]
    TorError(String),
}

/// Contains the configuration to communicate with the Tor Controler
/// As long as none of its clones are dropped, the hidden services created remain open
impl TorController {
    /// Creates a new TorTransport
    ///
    /// # Arguments
    ///
    /// * `url` - url to connect to the tor control. For example tcp://127.0.0.1:9051
    ///
    /// * `auth` - either authentication cookie bytes (32 byres) as hex in a string: assert_eq!(auth,"886b9177aec471965abd34b6a846dc32cf617dcff0625cba7a414e31dd4b75a0"), or a password as a quoted string: assert_eq!(auth,"\"mypassword\"")
    pub fn new(url: Url, auth: String) -> Result<Self, io::Error> {
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
            Err(err) => return Err(err),
        };
        Ok(Self { socket: Arc::new(socket), auth })
    }
    /// Creates an ephemeral hidden service listening on port, returns onion address
    pub fn create_ehs(&self, url: Url) -> Result<Url, TorError> {
        let local_socket = self.socket.try_clone()?;
        let mut stream = std::net::TcpStream::from(local_socket);

        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let host =
            url.host().ok_or(TorError::TorError("No host on url for listening".to_string()))?;
        let port =
            url.port().ok_or(TorError::TorError("No port on url for listening".to_string()))?;
        let payload = format!(
            "AUTHENTICATE {a}\r\nADD_ONION NEW:BEST Flags=DiscardPK Port={p},{h}:{p}\r\n",
            a = self.auth,
            p = port,
            h = host
        );
        stream.write_all(payload.as_bytes())?;
        stream.set_read_timeout(Some(Duration::from_secs(1)))?; // Maybe a bit too much. Gives tor time to reply
        let mut reader = BufReader::new(stream);
        let mut repl = String::new();
        while let Ok(nbytes) = reader.read_line(&mut repl) {
            if nbytes == 0 {
                break
            }
        }
        let re = Regex::new(r"250-ServiceID=(\w+*)")?;
        let cap: Result<regex::Captures<'_>, TorError> =
            re.captures(&repl).ok_or(TorError::TorError(repl.clone()));
        let hurl = cap?.get(1).map_or(Err(TorError::TorError(repl.clone())), |m| Ok(m.as_str()))?;
        let hurl = format!("tcp://{}.onion:{}", &hurl, port);
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
    /// * `control_info` - Possibility to open a control connection to create ephemeral hidden services that live as long as the TorTransport. Is a tuple of control url and authentication cookie as string (represented in hex)
    pub fn new(socks_url: Url, control_info: Option<(Url, String)>) -> Result<Self, TorError> {
        match control_info {
            Some(info) => {
                let (url, auth) = info;
                let tor_controller = Some(TorController::new(url, auth)?);
                Ok(Self { socks_url, tor_controller })
            }
            None => Ok(Self { socks_url, tor_controller: None }),
        }
    }

    pub fn create_ehs(&self, url: Url) -> Result<Url, TorError> {
        self.tor_controller
            .as_ref()
            .ok_or(TorError::TorError("No controller configured for this transport".to_string()))?
            .create_ehs(url)
    }

    pub async fn do_dial(self, url: Url) -> Result<Socks5Stream<TcpStream>, TorError> {
        let socks_url_str = self.socks_url.socket_addrs(|| None)?[0].to_string();
        let host = url.host().unwrap().to_string();
        let port = url.port().unwrap_or_else(|| 80);
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
        Ok(socket)
    }

    pub async fn do_listen(self, url: Url) -> Result<TcpListener, TorError> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }
}

impl Transport for TorTransport {
    type Acceptor = TcpListener;
    type Connector = Socks5Stream<TcpStream>;

    type Error = TorError;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor, Self::Error>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector, Self::Error>> + Send>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener, TransportError<Self::Error>> {
        if url.scheme() != "tcp" {
            return Err(TransportError::AddrNotSupported(url))
        }
        Ok(Box::pin(self.do_listen(url)))
    }

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>> {
        Ok(Box::pin(self.do_dial(url)))
    }
}
