/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 * Copyright (C) 2019 Arsenii "ark0f" Lyashenko (MIT License)
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
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    string::FromUtf8Error,
};

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use smol::{io::Cursor, net::UdpSocket};

/// The library's error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(
        #[from]
        #[source]
        io::Error,
    ),
    #[error("{0}")]
    FromUtf8(
        #[from]
        #[source]
        FromUtf8Error,
    ),
    #[error("Invalid SOCKS version: {0:x}")]
    InvalidVersion(u8),
    #[error("Invalid command: {0:x}")]
    InvalidCommand(u8),
    #[error("Invalid address type: {0:x}")]
    InvalidAtyp(u8),
    #[error("Invalid reserved bytes: {0:x}")]
    InvalidReserved(u8),
    #[error("Invalid authentication status: {0:x}")]
    InvalidAuthStatus(u8),
    #[error("Invalid authentication version of subnegotiation: {0:x}")]
    InvalidAuthSubnegotiation(u8),
    #[error("Invalid fragment id: {0:x}")]
    InvalidFragmentId(u8),
    #[error("Invalid authentication method: {0:?}")]
    InvalidAuthMethod(AuthMethod),
    #[error("SOCKS version is 4 when 5 is expected")]
    WrongVersion,
    #[error("No acceptable methods")]
    NoAcceptableMethods,
    #[error("Unsuccessful reply: {0:?}")]
    Response(UnsuccessfulReply),
    #[error("{0:?} length is more than 255 bytes")]
    TooLongString(StringKind),
}

/// Required to mark which string is too long.
/// See [`Error::TooLongString`].
///
/// [`Error::TooLongString`]: enum.Error.html#variant.TooLongString
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub enum StringKind {
    Domain,
    Username,
    Password,
}

/// The library's `Result` type alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

trait ReadExt: AsyncRead + Unpin {
    fn slice_to_u16_le(slice: &[u8]) -> u16 {
        assert_eq!(slice.len(), ::core::mem::size_of::<u16>());
        let mut res = 0;
        res |= slice[0] as u16;
        res |= (slice[1] as u16) << 8;
        res
    }

    async fn read_u8(&mut self) -> Result<u8> {
        let mut val = [0u8; 1];
        self.read_exact(&mut val[..]).await?;
        Ok(val[0])
    }

    async fn read_u16(&mut self) -> Result<u16> {
        let mut val = [0u8; 2];
        self.read_exact(&mut val[..]).await?;
        Ok(Self::slice_to_u16_le(&val))
    }

    async fn read_version(&mut self) -> Result<()> {
        let value = self.read_u8().await?;

        match value {
            0x04 => Err(Error::WrongVersion),
            0x05 => Ok(()),
            _ => Err(Error::InvalidVersion(value)),
        }
    }

    async fn read_method(&mut self) -> Result<AuthMethod> {
        let value = self.read_u8().await?;

        let method = match value {
            0x00 => AuthMethod::None,
            0x01 => AuthMethod::GssApi,
            0x02 => AuthMethod::UsernamePassword,
            0x03..=0x7f => AuthMethod::IanaReserved(value),
            0x80..=0xfe => AuthMethod::Private(value),
            0xff => return Err(Error::NoAcceptableMethods),
        };

        Ok(method)
    }

    async fn read_command(&mut self) -> Result<Command> {
        let value = self.read_u8().await?;

        let command = match value {
            0x01 => Command::Connect,
            0x02 => Command::Bind,
            0x03 => Command::UdpAssociate,
            _ => return Err(Error::InvalidCommand(value)),
        };

        Ok(command)
    }

    async fn read_atyp(&mut self) -> Result<Atyp> {
        let value = self.read_u8().await?;
        let atyp = match value {
            0x01 => Atyp::V4,
            0x03 => Atyp::Domain,
            0x04 => Atyp::V6,
            _ => return Err(Error::InvalidAtyp(value)),
        };
        Ok(atyp)
    }

    async fn read_reserved(&mut self) -> Result<()> {
        let value = self.read_u8().await?;

        match value {
            0x00 => Ok(()),
            _ => Err(Error::InvalidReserved(value)),
        }
    }

    async fn read_fragment_id(&mut self) -> Result<()> {
        let value = self.read_u8().await?;

        if value == 0x00 {
            Ok(())
        } else {
            Err(Error::InvalidFragmentId(value))
        }
    }

    async fn read_reply(&mut self) -> Result<()> {
        let value = self.read_u8().await?;

        let reply = match value {
            0x00 => return Ok(()),
            0x01 => UnsuccessfulReply::GeneralFailure,
            0x02 => UnsuccessfulReply::ConnectionNotAllowedByRules,
            0x03 => UnsuccessfulReply::NetworkUnreachable,
            0x04 => UnsuccessfulReply::HostUnreachable,
            0x05 => UnsuccessfulReply::ConnectionRefused,
            0x06 => UnsuccessfulReply::TtlExpired,
            0x07 => UnsuccessfulReply::CommandNotSupported,
            0x08 => UnsuccessfulReply::AddressTypeNotSupported,
            _ => UnsuccessfulReply::Unassigned(value),
        };

        Err(Error::Response(reply))
    }

    async fn read_target_addr(&mut self) -> Result<AddrKind> {
        let atyp: Atyp = self.read_atyp().await?;

        let addr = match atyp {
            Atyp::V4 => {
                let mut ip = [0; 4];
                self.read_exact(&mut ip).await?;
                let port = self.read_u16().await?;
                AddrKind::Ip(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip), port)))
            }
            Atyp::V6 => {
                let mut ip = [0; 16];
                self.read_exact(&mut ip).await?;
                let port = self.read_u16().await?;
                AddrKind::Ip(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(ip), port, 0, 0)))
            }
            Atyp::Domain => {
                let str = self.read_string().await?;
                let port = self.read_u16().await?;
                AddrKind::Domain(str, port)
            }
        };

        Ok(addr)
    }

    async fn read_string(&mut self) -> Result<String> {
        let len = self.read_u8().await?;
        let mut str = vec![0; len as usize];
        self.read_exact(&mut str).await?;
        let str = String::from_utf8(str)?;
        Ok(str)
    }

    async fn read_auth_version(&mut self) -> Result<()> {
        let value = self.read_u8().await?;

        if value != 0x01 {
            return Err(Error::InvalidAuthSubnegotiation(value));
        }

        Ok(())
    }

    async fn read_auth_status(&mut self) -> Result<()> {
        let value = self.read_u8().await?;

        if value != 0x00 {
            return Err(Error::InvalidAuthStatus(value));
        }

        Ok(())
    }

    async fn read_selection_msg(&mut self) -> Result<AuthMethod> {
        self.read_version().await?;
        self.read_method().await
    }

    async fn read_final(&mut self) -> Result<AddrKind> {
        self.read_version().await?;
        self.read_reply().await?;
        self.read_reserved().await?;
        let addr = self.read_target_addr().await?;
        Ok(addr)
    }
}

impl<T: AsyncRead + Unpin> ReadExt for T {}

trait WriteExt: AsyncWrite + Unpin {
    fn u16_to_array_le(val: u16) -> [u8; 2] {
        let mut res = [0u8; 2];
        res[0] = (val & 0xff) as u8;
        res[1] = ((val >> 8) & 0xff) as u8;
        res
    }

    async fn write_u8(&mut self, v: u8) -> Result<()> {
        Ok(self.write_all(&[v]).await?)
    }

    async fn write_u16(&mut self, v: u16) -> Result<()> {
        Ok(self.write_all(&Self::u16_to_array_le(v)).await?)
    }

    async fn write_version(&mut self) -> Result<()> {
        self.write_u8(0x05).await?;
        Ok(())
    }

    async fn write_method(&mut self, method: AuthMethod) -> Result<()> {
        let value = match method {
            AuthMethod::None => 0x00,
            AuthMethod::GssApi => 0x01,
            AuthMethod::UsernamePassword => 0x02,
            AuthMethod::IanaReserved(value) => value,
            AuthMethod::Private(value) => value,
        };
        self.write_u8(value).await?;
        Ok(())
    }

    async fn write_command(&mut self, command: Command) -> Result<()> {
        self.write_u8(command as u8).await?;
        Ok(())
    }

    async fn write_atyp(&mut self, atyp: Atyp) -> Result<()> {
        self.write_u8(atyp as u8).await?;
        Ok(())
    }

    async fn write_reserved(&mut self) -> Result<()> {
        self.write_u8(0x00).await?;
        Ok(())
    }

    async fn write_fragment_id(&mut self) -> Result<()> {
        self.write_u8(0x00).await?;
        Ok(())
    }

    async fn write_target_addr(&mut self, target_addr: &AddrKind) -> Result<()> {
        match target_addr {
            AddrKind::Ip(SocketAddr::V4(addr)) => {
                self.write_atyp(Atyp::V4).await?;
                self.write_all(&addr.ip().octets()).await?;
                self.write_u16(addr.port()).await?;
            }
            AddrKind::Ip(SocketAddr::V6(addr)) => {
                self.write_atyp(Atyp::V6).await?;
                self.write_all(&addr.ip().octets()).await?;
                self.write_u16(addr.port()).await?;
            }
            AddrKind::Domain(domain, port) => {
                self.write_atyp(Atyp::Domain).await?;
                self.write_string(domain, StringKind::Domain).await?;
                self.write_u16(*port).await?;
            }
        }
        Ok(())
    }

    async fn write_string(&mut self, string: &str, kind: StringKind) -> Result<()> {
        let bytes = string.as_bytes();
        if bytes.len() > 255 {
            return Err(Error::TooLongString(kind));
        }
        self.write_u8(bytes.len() as u8).await?;
        self.write_all(bytes).await?;
        Ok(())
    }

    async fn write_auth_version(&mut self) -> Result<()> {
        self.write_u8(0x01).await?;
        Ok(())
    }

    async fn write_methods(&mut self, methods: &[AuthMethod]) -> Result<()> {
        self.write_u8(methods.len() as u8).await?;
        for method in methods {
            self.write_method(*method).await?;
        }
        Ok(())
    }

    async fn write_selection_msg(&mut self, methods: &[AuthMethod]) -> Result<()> {
        self.write_version().await?;
        self.write_methods(methods).await?;
        self.flush().await?;
        Ok(())
    }

    async fn write_final(&mut self, command: Command, addr: &AddrKind) -> Result<()> {
        self.write_version().await?;
        self.write_command(command).await?;
        self.write_reserved().await?;
        self.write_target_addr(addr).await?;
        self.flush().await?;
        Ok(())
    }
}

impl<T: AsyncWrite + Unpin> WriteExt for T {}

async fn username_password_auth<S>(stream: &mut S, auth: Auth) -> Result<()>
where
    S: WriteExt + ReadExt + Send,
{
    stream.write_auth_version().await?;
    stream.write_string(&auth.username, StringKind::Username).await?;
    stream.write_string(&auth.password, StringKind::Password).await?;
    stream.flush().await?;

    stream.read_auth_version().await?;
    stream.read_auth_status().await
}

async fn init<S, A>(
    stream: &mut S,
    command: Command,
    addr: A,
    auth: Option<Auth>,
) -> Result<AddrKind>
where
    S: WriteExt + ReadExt + Send,
    A: Into<AddrKind>,
{
    let addr: AddrKind = addr.into();

    let mut methods = Vec::with_capacity(2);
    methods.push(AuthMethod::None);
    if auth.is_some() {
        methods.push(AuthMethod::UsernamePassword);
    }
    stream.write_selection_msg(&methods).await?;

    let method: AuthMethod = stream.read_selection_msg().await?;
    match method {
        AuthMethod::None => {}
        // FIXME: until if let in match is stabilized
        AuthMethod::UsernamePassword if auth.is_some() => {
            username_password_auth(stream, auth.unwrap()).await?;
        }
        _ => return Err(Error::InvalidAuthMethod(method)),
    }

    stream.write_final(command, &addr).await?;
    stream.read_final().await
}

/// Required for a username + password authentication.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Auth {
    pub username: String,
    pub password: String,
}

impl Auth {
    /// Constructs `Auth` with the specified username and a password.
    pub fn new<U, P>(username: U, password: P) -> Self
    where
        U: Into<String>,
        P: Into<String>,
    {
        Self { username: username.into(), password: password.into() }
    }
}

/// A proxy authentication method.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub enum AuthMethod {
    /// No authentication required.
    None,
    /// GSS API.
    GssApi,
    /// A username + password authentication.
    UsernamePassword,
    /// IANA reserved.
    IanaReserved(u8),
    /// A private authentication method.
    Private(u8),
}

enum Command {
    Connect = 0x01,
    Bind = 0x02,
    UdpAssociate = 0x03,
}

enum Atyp {
    V4 = 0x01,
    Domain = 0x03,
    V6 = 0x4,
}

/// An unsuccessful reply from a proxy server.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub enum UnsuccessfulReply {
    GeneralFailure,
    ConnectionNotAllowedByRules,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
    Unassigned(u8),
}

/// Either [`SocketAddr`] or a domain and a port.
///
/// [`SocketAddr`]: https://doc.rust-lang.org/std/net/enum.SocketAddr.html
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum AddrKind {
    Ip(SocketAddr),
    Domain(String, u16),
}

impl AddrKind {
    const MAX_SIZE: usize = 1 // atyp
                          + 1 // domain len
                          + 255 // domain
                          + 2; // port

    // FIXME: until ToSocketAddrs is allowed to implement
    fn to_socket_addr(&self) -> String {
        match self {
            AddrKind::Ip(addr) => addr.to_string(),
            AddrKind::Domain(domain, port) => format!("{}:{}", domain, port),
        }
    }

    fn size(&self) -> usize {
        1 + // atyp
        2 + // port
        match self {
            AddrKind::Ip(SocketAddr::V4(_)) => 4,
            AddrKind::Ip(SocketAddr::V6(_)) => 16,
            AddrKind::Domain(domain, _) => 1 /* string len */ + domain.len(),
        }
    }
}

impl From<(IpAddr, u16)> for AddrKind {
    fn from(value: (IpAddr, u16)) -> Self {
        Self::Ip(value.into())
    }
}

impl From<(Ipv4Addr, u16)> for AddrKind {
    fn from(value: (Ipv4Addr, u16)) -> Self {
        Self::Ip(value.into())
    }
}

impl From<(Ipv6Addr, u16)> for AddrKind {
    fn from(value: (Ipv6Addr, u16)) -> Self {
        Self::Ip(value.into())
    }
}

impl From<(String, u16)> for AddrKind {
    fn from((domain, port): (String, u16)) -> Self {
        Self::Domain(domain, port)
    }
}

impl From<(&'_ str, u16)> for AddrKind {
    fn from((domain, port): (&'_ str, u16)) -> Self {
        Self::Domain(domain.to_owned(), port)
    }
}

impl From<SocketAddr> for AddrKind {
    fn from(value: SocketAddr) -> Self {
        Self::Ip(value)
    }
}

impl From<SocketAddrV4> for AddrKind {
    fn from(value: SocketAddrV4) -> Self {
        Self::Ip(value.into())
    }
}

impl From<SocketAddrV6> for AddrKind {
    fn from(value: SocketAddrV6) -> Self {
        Self::Ip(value.into())
    }
}

/// Proxifies a TCP connection. Performs the [`CONNECT`] command under the hood.
///
/// [`CONNECT`]: https://tools.ietf.org/html/rfc1928#page-6
///
/// ```no_run
/// let stream = TcpStream::connect("my-proxy-server.com:54321").await?;
/// let mut stream = BufStream::new(stream);
/// connect(&mut stream, ("google.com", 80), None).await?;
/// ```
pub async fn connect<S, A>(socket: &mut S, addr: A, auth: Option<Auth>) -> Result<AddrKind>
where
    S: AsyncWrite + AsyncRead + Send + Unpin,
    A: Into<AddrKind>,
{
    init(socket, Command::Connect, addr, auth).await
}

/// A listener that accepts TCP connections through a proxy.
///
/// ```no_run
/// let stream = TcpStream::connect("my-proxy-server.com:54321").await?;
/// let mut stream = BufStream::new(stream);
/// let (stream, addr) = SocksListener::bind(stream, ("ftp-server.org", 21), None)
///     .await?
///     .accept()
///     .await?;
/// ```
#[derive(Debug)]
pub struct SocksListener<S> {
    stream: S,
    proxy_addr: AddrKind,
}

impl<S> SocksListener<S>
where
    S: AsyncWrite + AsyncRead + Send + Unpin,
{
    /// Creates `SocksListener`. Performs the [`BIND`] command under the hood.
    ///
    /// [`BIND`]: https://tools.ietf.org/html/rfc1928#page-6
    pub async fn bind<A>(mut stream: S, addr: A, auth: Option<Auth>) -> Result<Self>
    where
        A: Into<AddrKind>,
    {
        let addr = init(&mut stream, Command::Bind, addr, auth).await?;
        Ok(Self { stream, proxy_addr: addr })
    }

    pub fn proxy_addr(&self) -> &AddrKind {
        &self.proxy_addr
    }

    pub async fn accept(mut self) -> Result<(S, AddrKind)> {
        let addr = self.stream.read_final().await?;
        Ok((self.stream, addr))
    }
}

/// A UDP socket that sends packets through a proxy.
#[derive(Debug)]
pub struct SocksDatagram<S> {
    socket: UdpSocket,
    proxy_addr: AddrKind,
    stream: S,
}

impl<S> SocksDatagram<S>
where
    S: AsyncWrite + AsyncRead + Send + Unpin,
{
    /// Creates `SocksDatagram`. Performs [`UDP ASSOCIATE`] under the hood.
    ///
    /// [`UDP ASSOCIATE`]: https://tools.ietf.org/html/rfc1928#page-7
    pub async fn associate<A>(
        mut proxy_stream: S,
        socket: UdpSocket,
        auth: Option<Auth>,
        association_addr: Option<A>,
    ) -> Result<Self>
    where
        A: Into<AddrKind>,
    {
        let addr = association_addr
            .map(Into::into)
            .unwrap_or_else(|| AddrKind::Ip(SocketAddr::new(IpAddr::from([0, 0, 0, 0]), 0)));
        let proxy_addr = init(&mut proxy_stream, Command::UdpAssociate, addr, auth).await?;
        socket.connect(proxy_addr.to_socket_addr()).await?;
        Ok(Self { socket, proxy_addr, stream: proxy_stream })
    }

    pub fn proxy_addr(&self) -> &AddrKind {
        &self.proxy_addr
    }

    pub fn get_ref(&self) -> &UdpSocket {
        &self.socket
    }

    pub fn get_mut(&mut self) -> &mut UdpSocket {
        &mut self.socket
    }

    pub fn into_inner(self) -> (S, UdpSocket) {
        (self.stream, self.socket)
    }

    async fn write_request(buf: &[u8], addr: AddrKind) -> Result<Vec<u8>> {
        let bytes_size = Self::get_buf_size(addr.size(), buf.len());
        let bytes = Vec::with_capacity(bytes_size);

        let mut cursor = Cursor::new(bytes);
        cursor.write_reserved().await?;
        cursor.write_reserved().await?;
        cursor.write_fragment_id().await?;
        cursor.write_target_addr(&addr).await?;
        cursor.write_all(buf).await?;

        let bytes = cursor.into_inner();
        Ok(bytes)
    }

    pub async fn send_to<A>(&self, buf: &[u8], addr: A) -> Result<usize>
    where
        A: Into<AddrKind>,
    {
        let addr: AddrKind = addr.into();
        let bytes = Self::write_request(buf, addr).await?;
        Ok(self.socket.send(&bytes).await?)
    }

    async fn read_response(
        len: usize,
        buf: &mut [u8],
        bytes: &mut [u8],
    ) -> Result<(usize, AddrKind)> {
        let mut cursor = Cursor::new(bytes);
        cursor.read_reserved().await?;
        cursor.read_reserved().await?;
        cursor.read_fragment_id().await?;
        let addr = cursor.read_target_addr().await?;
        let header_len = cursor.position() as usize;
        cursor.read_exact(buf).await?;
        Ok((len - header_len, addr))
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, AddrKind)> {
        let bytes_size = Self::get_buf_size(AddrKind::MAX_SIZE, buf.len());
        let mut bytes = vec![0; bytes_size];

        let len = self.socket.recv(&mut bytes).await?;
        let (read, addr) = Self::read_response(len, buf, &mut bytes).await?;
        Ok((read, addr))
    }

    fn get_buf_size(addr_size: usize, buf_len: usize) -> usize {
        2 // reserved
        + 1 // fragment id
        + addr_size
        + buf_len
    }
}
