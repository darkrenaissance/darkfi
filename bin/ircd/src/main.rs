#[macro_use]
extern crate clap;
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

use async_executor::Executor;
use async_std::io::BufReader;
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, Future, FutureExt,
};
use log::{debug, error, info, warn};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use smol::Async;

use drk::{
    net,
    serial::{Decodable, Encodable},
    Error, Result,
};

mod privmsg;
mod program_options;
mod protocol_privmsg;
mod irc_server;

use crate::privmsg::PrivMsg;
use crate::program_options::ProgramOptions;
use crate::protocol_privmsg::ProtocolPrivMsg;
use crate::irc_server::IrcServerConnection;

async fn process(
    recvr: async_channel::Receiver<Arc<PrivMsg>>,
    stream: Async<TcpStream>,
    peer_addr: SocketAddr,
    p2p: net::P2pPtr,
    executor: Arc<Executor<'_>>,
) -> Result<()> {
    let (reader, writer) = stream.split();

    let mut reader = BufReader::new(reader);
    let mut connection = IrcServerConnection::new(writer);

    loop {
        let mut line = String::new();
        futures::select! {
            privmsg = recvr.recv().fuse() => {
                let privmsg = privmsg.expect("internal message queue error");
                debug!("ABOUT TO SEND {:?}", privmsg);
                let irc_msg = format!(
                    ":{}!darkfi@127.0.0.1 PRIVMSG {} :{}\n",
                    privmsg.nickname,
                    privmsg.channel,
                    privmsg.message
                );

                connection.reply(&irc_msg).await?;
            }
            err = reader.read_line(&mut line).fuse() => {
                if let Err(err) = err {
                    warn!("Read line error. Closing stream for {}: {}", peer_addr, err);
                    return Ok(())
                }
                process_user_input(line, peer_addr, &mut connection, p2p.clone()).await;
            }
        };
    }
}

async fn process_user_input(
    mut line: String,
    peer_addr: SocketAddr,
    connection: &mut IrcServerConnection,
    p2p: net::P2pPtr,
) {
    if line.len() == 0 {
        warn!("Received empty line from {}. Closing connection.", peer_addr);
        return
    }
    assert!(&line[(line.len() - 1)..] == "\n");
    // Remove the \n character
    line.pop();

    debug!("Received '{}' from {}", line, peer_addr);

    if let Err(err) = connection.update(line, p2p.clone()).await {
        warn!("Connection error: {} for {}", err, peer_addr);
        return
    }
}

async fn channel_loop(
    p2p: net::P2pPtr,
    sender: async_channel::Sender<Arc<PrivMsg>>,
    executor: Arc<Executor<'_>>,
) -> Result<()> {
    debug!("CHANNEL SUBS LOOP");
    let new_channel_sub = p2p.subscribe_channel().await;

    loop {
        let channel = new_channel_sub.receive().await?;

        debug!("NEWCHANNEL");

        let protocol_privmsg = ProtocolPrivMsg::new(channel, sender.clone()).await;
        protocol_privmsg.start(executor.clone()).await;
    }
}

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let listener = match Async::<TcpListener>::bind(options.irc_accept_addr) {
        Ok(listener) => listener,
        Err(err) => {
            error!("Bind listener failed: {}", err);
            return Err(Error::OperationFailed)
        }
    };
    let local_addr = match listener.get_ref().local_addr() {
        Ok(addr) => addr,
        Err(err) => {
            error!("Failed to get local address: {}", err);
            return Err(Error::OperationFailed)
        }
    };
    info!("Listening on {}", local_addr);

    let p2p = net::P2p::new(options.network_settings);
    // Performs seed session
    p2p.clone().start(executor.clone()).await?;
    // Actual main p2p session
    let ex2 = executor.clone();
    let p2p2 = p2p.clone();
    executor
        .spawn(async move {
            if let Err(err) = p2p2.run(ex2).await {
                error!("Error: p2p run failed {}", err);
            }
        })
        .detach();

    let (sender, recvr) = async_channel::unbounded();
    // for now the p2p and channel sub sessions just run forever
    // so detach them as background processes.
    executor.spawn(channel_loop(p2p.clone(), sender, executor.clone())).detach();

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::ServiceStopped)
            }
        };
        info!("Accepted client: {}", peer_addr);

        let p2p2 = p2p.clone();
        let ex2 = executor.clone();
        executor.spawn(process(recvr.clone(), stream, peer_addr, p2p2, ex2)).detach();
    }
}

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let options = ProgramOptions::load()?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(ex.clone(), options)))
}
