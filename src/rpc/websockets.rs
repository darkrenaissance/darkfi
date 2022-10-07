use std::{
    net::{TcpStream, ToSocketAddrs},
    pin::Pin,
    task::{Context, Poll},
};

use async_tungstenite::WebSocketStream;
use futures::sink::Sink;
use futures_rustls::{client::TlsStream, rustls::ServerName, TlsConnector};
use smol::{prelude::*, Async};
use tungstenite::{handshake::client::Response, Message};
use url::Url;

use crate::{Error, Result as DrkResult};

#[allow(clippy::large_enum_variant)]
pub enum WsStream {
    Tcp(WebSocketStream<Async<TcpStream>>),
    Tls(WebSocketStream<TlsStream<Async<TcpStream>>>),
}

impl Sink<Message> for WsStream {
    type Error = tungstenite::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            WsStream::Tcp(s) => Pin::new(s).poll_ready(cx),
            WsStream::Tls(s) => Pin::new(s).poll_ready(cx),
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        match &mut *self {
            WsStream::Tcp(s) => Pin::new(s).start_send(item),
            WsStream::Tls(s) => Pin::new(s).start_send(item),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            WsStream::Tcp(s) => Pin::new(s).poll_flush(cx),
            WsStream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut *self {
            WsStream::Tcp(s) => Pin::new(s).poll_close(cx),
            WsStream::Tls(s) => Pin::new(s).poll_close(cx),
        }
    }
}

impl Stream for WsStream {
    type Item = tungstenite::Result<Message>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut *self {
            WsStream::Tcp(s) => Pin::new(s).poll_next(cx),
            WsStream::Tls(s) => Pin::new(s).poll_next(cx),
        }
    }
}

/// Connects to a WebSocket address (optionally secured by TLS).
pub async fn connect(addr: &str, tls: TlsConnector) -> DrkResult<(WsStream, Response)> {
    let url = Url::parse(addr)?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::UrlParse(format!("Missing host in {}", url)))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| Error::UrlParse(format!("Missing port in {}", url)))?;

    let socket_addr = {
        let host = host.clone();
        smol::unblock(move || (host.as_str(), port).to_socket_addrs())
            .await?
            .next()
            .ok_or(Error::NoUrlFound)?
    };

    match url.scheme() {
        "ws" => {
            let stream = Async::<TcpStream>::connect(socket_addr).await?;
            let (stream, resp) = async_tungstenite::client_async(addr, stream).await?;
            Ok((WsStream::Tcp(stream), resp))
        }
        "wss" => {
            let stream = Async::<TcpStream>::connect(socket_addr).await?;
            let stream = tls.connect(ServerName::try_from(host.as_str())?, stream).await?;
            let (stream, resp) = async_tungstenite::client_async(addr, stream).await?;
            Ok((WsStream::Tls(stream), resp))
        }
        scheme => Err(Error::UrlParse(format!("Invalid url scheme `{}`, in `{}`", scheme, url))),
    }
}
