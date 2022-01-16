use std::{fs::*, path::PathBuf, time::Instant};

use crate::{
    crypto::proof::{ProvingKey, VerifyingKey},
    util::serial::Decodable,
    zk::{circuit::MintContract, vm},
    Result,
};

pub struct ContractLoader {}

impl ContractLoader {
    // let bin = from_bincode("Mint", "proofs/mint.zk.bin");
    pub fn from_bincode(name: String, path: PathBuf) -> Result<vm::ZkContract> {
        let start = Instant::now();
        let file = File::open(path)?;
        let zkbin = vm::ZkBinary::decode(file)?;
        for contract_name in zkbin.contracts.keys() {
            println!("Loaded '{}' contract.", contract_name);
        }
        println!("Load time: [{:?}]", start.elapsed());
        let contract = zkbin.contracts[&name].clone();
        Ok(contract)
    }

    // let prk = load_prk("Mint", "proofs/mint.zk/bin", "proofs/mint.prk")
    pub fn load_prk(
        contract_name: String,
        contract_path: PathBuf,
        key_path: PathBuf,
    ) -> Result<()> {
        // TODO: benchmarks
        //let start = Instant::now();
        let contract_data = metadata(contract_path)?;
        let key_data = metadata(key_path)?;

        let contract_time = contract_data.modified()?;
        let key_time = key_data.modified()?;

        if key_time > contract_time {
            println!("Key is newer than contract.");
        } else {
            Self::create_prk(contract_name)?;
        }
        Ok(())
    }

    // let prk = load_vrk("Mint", "proofs/mint.zk/bin", "proofs/mint.vrk")
    pub fn load_vrk(
        contract_name: String,
        contract_path: PathBuf,
        key_path: PathBuf,
    ) -> Result<()> {
        // TODO: benchmarks
        //let start = Instant::now();
        let contract_data = metadata(contract_path)?;
        let key_data = metadata(key_path)?;

        let contract_time = contract_data.modified()?;
        let key_time = key_data.modified()?;

        if key_time > contract_time {
            println!("Key is newer than contract.");
        } else {
            Self::create_vrk(contract_name)?;
        }
        Ok(())
    }

    pub fn create_prk(name: String) -> Result<()> {
        // TODO: implement this
        let _mint_pk = ProvingKey::build(11, MintContract::default());
        let _file = File::create(name + ".prk")?;
        // TODO: serialize and save file
        Ok(())
    }

    pub fn create_vrk(name: String) -> Result<()> {
        // TODO: implement this
        let _mint_vk = VerifyingKey::build(11, MintContract::default());
        let _file = File::create(name + ".vrk")?;
        // TODO: serialize and save file
        Ok(())
    }
}
