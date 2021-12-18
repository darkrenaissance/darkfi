use async_std::sync::{Arc, Mutex};

use drk::darkpulse::{
    dbsql, net::messages, utility, CiphertextHash, cli_option::CliOption, control_message::ControlCommand, MemPool,
    slabs_manager::SlabsManager,
};

use drk::Result;

use async_executor::Executor;
use easy_parallel::Parallel;
use log::*;
use drk::net::P2p;
use smol::Unblock;

async fn on_receive_slab(
    p2p: Arc<P2p>,
    slab_rx: async_channel::Receiver<CiphertextHash>,
) -> Result<()> {
    loop {
        let slab = slab_rx.recv().await?;
        p2p.broadcast(messages::InvMessage {
            slabs_hash: vec![slab],
        })
        .await?;
        }
}

async fn start(executor: Arc<Executor<'_>>, options: CliOption, db: dbsql::Dbsql) -> Result<()> {
    let p2p = P2p::new(options.network_settings);

    p2p.clone().start(executor.clone()).await?;

    let p2p_run_task = executor.spawn(p2p.clone().run(executor.clone()));

    let _mem_pool: MemPool = Arc::new(Mutex::new(vec![]));

    // choose a channel
    if let Some(new_channel) = options.new_channel {
        info!(
            "channel added with the name {}",
            new_channel.get_channel_name()
        );
        db.add_channel(&new_channel).unwrap();
    }
    let main_channel = utility::choose_channel(&db, options.channel_name)?;
    let channels = db.get_channels()?;
    let username = utility::setup_username(options.username, &db)?;

    let (slab_sx, slab_rx) = async_channel::unbounded::<CiphertextHash>();

    let slabman = SlabsManager::new(db, slab_sx, main_channel.clone()).await;

    let subscribtion = p2p.subscribe_channel().await;

    let executor2 = executor.clone();
    let setup_channels_task = executor2.clone().spawn(async move {
        loop {
            let network_channel = subscribtion.receive().await.unwrap();
            utility::setup_network_channel(executor2.clone(), network_channel, slabman.clone())
                .await;
            }
    });

    let receive_slab = executor.spawn(on_receive_slab(p2p.clone(), slab_rx.clone()));

    p2p.broadcast(messages::SyncMessage {}).await?;

    let stdin = Unblock::new(std::io::stdin());
    let mut stdin = futures::io::BufReader::new(stdin);

    loop {
        println!("[1] Send Message");
        println!("[2] Send Sync");
        println!("[3] list available channels");
        println!("[4] show the channel address");
        println!("[5] Quit");

        let buf = utility::read_line(&mut stdin).await?;

        match &buf[..] {
            "1" => {
                let slab = utility::pack_slab(
                    &main_channel.get_channel_secret(),
                    username.clone(),
                    String::from("Hello"),
                    ControlCommand::Message,
                )
                    .await?;

                p2p.broadcast(slab).await?;
            }
            "2" => {
                p2p.broadcast(messages::SyncMessage {}).await?;
            }
            "3" => {
                println!("------------------");
                println!("Available channels:");
                for channel in channels.iter() {
                    println!("- {}", channel.get_channel_name());
                }
                println!("NOTE: switch with one of the available channels by using --channel flag");
                println!("------------------");
            }
            "4" => {
                println!("------------------");
                println!("Address: {}", main_channel.get_channel_address());
                println!("------------------");
            }
            "5" => break,
            _ => {}
        }
    }

    setup_channels_task.cancel().await;
    p2p_run_task.cancel().await;
    receive_slab.cancel().await;
    Ok(())
}

pub fn main() -> Result<()> {
    use simplelog::*;

    let cli_option = CliOption::get()?;

    let debug_level = if cli_option.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    CombinedLogger::init(vec![
        TermLogger::new(debug_level, Config::default(), TerminalMode::Mixed, ColorChoice::Always),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create("/tmp/darkpulsenode.log").unwrap(),
        ),
    ])
        .unwrap();

    let mut db = dbsql::Dbsql::new()?;
    db.start()?;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, cli_option, db).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
