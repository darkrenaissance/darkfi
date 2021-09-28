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
        60, 233, 151, 189, 143, 1, 111, 173, 241, 1, 171, 31, 123, 156, 160, 235, 32, 108, 157, 75,
        100, 150, 255, 154, 36, 254, 230, 97, 83, 248, 213, 223, 183, 146, 221, 49, 146, 156, 140,
        27, 196, 234, 193, 229, 174, 93, 126, 232, 9, 85, 58, 45, 95, 105, 168, 167, 153, 123, 246,
        110, 193, 70, 192, 186,
    ])
    .unwrap();
    let bridge = bridge::Bridge::new();

    println!("main keypair {:?}", main_keypair.to_bytes());
    println!("main pubkey {}", main_keypair.pubkey().to_string());

    let network = drk::service::NetworkName::Solana;

    let sol_client = SolClient::new(serialize(&main_keypair), "devnet").await?;

    bridge
        .clone()
        .add_clients(network.clone(), sol_client)
        .await?;

    let bridge2 = bridge.clone();
    let bridge_subscribtion = bridge2.subscribe().await;

    bridge_subscribtion
        .sender
        .send(bridge::BridgeRequests {
            network: network.clone(),
            payload: bridge::BridgeRequestsPayload::Watch(None),
        })
        .await?;

    let bridge_res = bridge_subscribtion.receiver.recv().await?;

    match bridge_res.payload {
        bridge::BridgeResponsePayload::Watch(_, token_pub) => {
            println!("watch this address {}", token_pub);
        }
        _ => {}
    }

    let bridge_subscribtion = bridge.subscribe().await;

    bridge_subscribtion
        .sender
        .send(bridge::BridgeRequests {
            network: network.clone(),
            payload: bridge::BridgeRequestsPayload::Watch(None),
        })
        .await?;

    let bridge_res = bridge_subscribtion.receiver.recv().await?;

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
