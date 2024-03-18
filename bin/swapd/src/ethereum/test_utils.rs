use std::{path::Path, sync::Arc, time::Duration};

use ethers::{core::utils::Anvil, prelude::*, solc::Solc, utils::AnvilInstance};

/// Starts a local anvil instance and deploys the `SwapCreator` contract to it.
///
/// Returns the contract address, provider, wallet, and anvil instance.
///
/// # Panics
///
/// - if the contract cannot be found in the expected path
/// - if the contract cannot be compiled
/// - if the provider fails to connect to the anvil instance
/// - if the contract fails to deploy
#[allow(dead_code)]
pub(crate) async fn deploy_swap_creator() -> (Address, Arc<Provider<Ws>>, LocalWallet, AnvilInstance)
{
    // compile contract for testing
    let source = Path::new(&env!("CARGO_MANIFEST_DIR")).join("ethereum/src/SwapCreator.sol");
    let input = CompilerInput::new(source.clone()).unwrap().first().unwrap().clone();
    let compiled = Solc::default().compile(&input).expect("could not compile contract");
    assert!(compiled.errors.is_empty(), "errors: {:?}", compiled.errors);

    let (abi, bytecode, _) =
        compiled.find("SwapCreator").expect("could not find contract").into_parts_or_default();

    // setup anvil and signing wallet
    let anvil = Anvil::new().spawn();
    let wallet: LocalWallet = anvil.keys()[0].clone().into();
    let provider = Arc::new(
        Provider::<Ws>::connect(anvil.ws_endpoint())
            .await
            .unwrap()
            .interval(Duration::from_millis(10u64)),
    );
    let signer =
        SignerMiddleware::new(provider.clone(), wallet.clone().with_chain_id(anvil.chain_id()));

    // deploy contract
    let factory = ContractFactory::new(abi, bytecode, signer.into());
    let contract = factory.deploy(()).unwrap().send().await.unwrap();
    let contract_address = contract.address();

    (contract_address, provider, wallet.with_chain_id(anvil.chain_id()), anvil)
}
