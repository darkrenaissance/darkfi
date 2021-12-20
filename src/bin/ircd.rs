use async_executor::Executor;
use async_std::io::BufReader;
use futures::{AsyncBufReadExt, AsyncWriteExt, Future, FutureExt};
use log::{info, error, warn};
use std::io;
use smol::Async;
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

use drk::{Error, Result};

/*
NICK fifififif
USER username 0 * :Real
:behemoth 001 fifififif :Hi, welcome to IRC
:behemoth 002 fifififif :Your host is behemoth, running version miniircd-2.1
:behemoth 003 fifififif :This server was created sometime
:behemoth 004 fifififif behemoth miniircd-2.1 o o
:behemoth 251 fifififif :There are 1 users and 0 services on 1 server
:behemoth 422 fifififif :MOTD File is missing
JOIN #dev
:fifififif!username@127.0.0.1 JOIN #dev
:behemoth 331 fifififif #dev :No topic is set
:behemoth 353 fifififif = #dev :fifififif
:behemoth 366 fifififif #dev :End of NAMES list
PRIVMSG #dev hihi
*/

async fn process(mut stream: Async<TcpStream>, peer_addr: SocketAddr) {
    //stream.write_all(b":behemoth 001 fifififif :Hi, welcome to IRC").await;
    //stream.write_all(b"NICK fofofofofo");
    //stream.write_all(b"USER username 0 * :Real");
    //stream.write_all(b"JOIN #dev");
    //stream.write_all(b"PRIVMSG #dev y0");

    // PING :behemoth

    let mut reader = BufReader::new(stream);

    loop {
        let mut line = String::new();
        if let Err(err) = reader.read_line(&mut line).await {
            warn!("Read line ended. Closing stream for {}", peer_addr);
            return;
        }
        if line.len() == 0 {
            warn!("Received empty line from {}. Closing connection.", peer_addr);
            return;
        }
        assert!(&line[(line.len() - 1)..] == "\n");
        // Remove the \n character
        line.pop();
        println!("Recv: {}", line);
    }
}

async fn async_main(executor: Arc<Executor<'_>>) -> Result<()> {
    let accept_addr = ([127, 0, 0, 1], 6667);
    let listener = match Async::<TcpListener>::bind(accept_addr) {
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

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::ServiceStopped)
            }
        };
        info!("Accepted client: {}", peer_addr);

        executor.spawn(process(stream, peer_addr)).detach();
    }
}

fn main() -> Result<()> {
    simple_logger::init_with_level(log::Level::Trace)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(async_main(ex.clone())))
        //let acceptor = Acceptor::new();
        //let listener = match Async::<TcpListener>::bind(accept_addr) {
        //    Ok(listener) => listener,
        //    Err(err) => {
        //        error!("Bind listener failed: {}", err);
        //        return Err(Error::OperationFailed)
        //    }
        //};
        //let local_addr = match listener.get_ref().local_addr() {
        //    Ok(addr) => addr,
        //    Err(err) => {
        //        error!("Failed to get local address: {}", err);
        //        return Err(Error::OperationFailed)
        //    }
        //};
}

