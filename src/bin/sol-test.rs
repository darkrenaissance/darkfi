use clap::clap_app;
use drk::serial::serialize;
use drk::service::bridge;
use drk::Result;

#[cfg(feature = "sol")]
async fn run() -> Result<()> {
    use drk::service::SolClient;

    use solana_sdk::{signature::Signer, signer::keypair::Keypair};

    let main_keypair: Keypair;
    main_keypair = Keypair::from_bytes(&[
        80, 20, 135, 18, 220, 115, 132, 178, 122, 1, 195, 250, 182, 241, 4, 82, 88, 12, 232, 249,
        154, 105, 222, 221, 229, 138, 22, 232, 197, 151, 155, 28, 173, 182, 189, 174, 66, 196, 63,
        98, 201, 68, 203, 60, 72, 93, 179, 244, 39, 158, 223, 249, 102, 160, 217, 245, 24, 153,
        152, 52, 41, 248, 226, 32,
    ])
    .unwrap();
    let bridge = bridge::Bridge::new();

    println!("main pubkey {}", main_keypair.pubkey().to_string());

    let network = String::from("sol");

    let sol_client = SolClient::new(serialize(&main_keypair)).await?;

    bridge
        .clone()
        .add_clients(network.clone(), sol_client)
        .await?;

    let bridge_subscribtion = bridge.subscribe().await;

    bridge_subscribtion
        .sender
        .send(bridge::BridgeRequests {
            network: network.clone(),
            payload: bridge::BridgeRequestsPayload::Watch(None),
        })
        .await?;

    let bridge_res = bridge_subscribtion.receiver.recv().await?;

    // XXX this will not work
    match bridge_res.payload {
        bridge::BridgeResponsePayload::Watch(_, token_pub) => {
            println!("watch this address {}", token_pub);
        }
        _ => {}
    }

    async_std::task::sleep(std::time::Duration::from_secs(3600)).await;

    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "sol")]
    {
        let args = clap_app!(darkfid =>
            (@arg verbose: -v --verbose "Increase verbosity")
        )
        .get_matches();

        let loglevel = if args.is_present("verbose") {
            log::Level::Debug
        } else {
            log::Level::Info
        };

        simple_logger::init_with_level(loglevel)?;
    }

    #[cfg(feature = "sol")]
    smol::block_on(run())?;
    Ok(())
}
