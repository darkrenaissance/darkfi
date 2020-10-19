#[macro_use]
extern crate clap;
use bls12_381::Scalar;
use sapvi::{BlsStringConversion, Decodable, Encodable, ZKContract, ZKProof};
use simplelog::*;
use std::fs;
use std::fs::File;
use std::time::Instant;
//use log::*;

type Result<T> = std::result::Result<T, failure::Error>;

// do the setup for mint.zcd, save the params in mint.setup
fn trusted_setup(contract_data: String, setup_file: String) -> Result<()> {
    let start = Instant::now();
    let file = File::open("jubjub.zcd")?;
    let mut contract = ZKContract::decode(file)?;
    println!(
        "loaded contract '{}': [{:?}]",
        contract.name,
        start.elapsed()
    );
    println!("Stats:");
    println!("    Constants: {}", contract.vm.constants.len());
    println!("    Alloc: {}", contract.vm.alloc.len());
    println!("    Operations: {}", contract.vm.ops.len());
    println!("    Constraint Instructions: {}",
        contract.vm.constraints.len()
    );
    contract.setup("jubjub.zts")?;
    Ok(())
}

// make the proof
fn create_proof(
    contract_data: String,
    setup_file: String,
    params: String,
    zk_proof: String,
) -> Result<()> {
    let start = Instant::now();
    let file = File::open("jubjub.zcd")?;
    let mut contract = ZKContract::decode(file)?;
    println!(
        "Loaded contract '{}': [{:?}]",
        contract.name,
        start.elapsed()
    );
    contract.load_setup("jubjub.zts")?;
    let param_content = fs::read_to_string(params).expect("something went wrong reading the file");
    let lines: Vec<&str> = param_content.lines().collect();
    for line in lines {
        let name = line.split_whitespace().next().unwrap_or("");
        let value = line.trim_start_matches(name).trim_left();
        contract.set_param(name, Scalar::from_string(value));
    }
    let proof = contract.prove()?;
    let mut file = File::create("jubjub.prf")?;
    proof.encode(&mut file)?;
    Ok(())
}

//verify the proof
fn verify_proof(contract_data: String, setup_file: String, zk_proof: String) -> Result<()> {
    let start = Instant::now();
    let proof_file = File::open("jubjub.prf")?;
    let proof = ZKProof::decode(proof_file)?;
    let contract_file = File::open("jubjub.zcd")?;
    let mut contract = ZKContract::decode(contract_file)?;
    contract.load_setup("jubjub.zts")?;
    assert!(contract.verify(&proof));
    Ok(())
}

// show public values in proof
fn show_public(zk_proof: String) -> Result<()> {
    let start = Instant::now();
    let file = File::open("jubjub.prf")?;
    let proof = ZKProof::decode(file)?;
    assert_eq!(proof.public.len(), 2);
    println!("Public values: {:?}", proof.public);
    Ok(())
}

fn main() -> Result<()> {
    let matches = clap_app!(zkvm =>
        (version: "0.1.0")
        (author: "Rose O'Leary <rrose@tuta.io>")
        (about: "Zero Knowledge Virtual Machine Command Line Interface")
        (@subcommand init =>
            (about: "Trusted setup phase")
            (@arg CONTRACT_DATA: +required "Input zero-knowledge contract data (.zcd)")
            (@arg SETUP_FILE: +required "Output setup parameters")
        )
        (@subcommand prove =>
            (about: "Create zero-knowledge proof")
            (@arg CONTRACT_DATA: +required "Input zero-knowledge contract data (.zcd)")
            (@arg SETUP_FILE: +required "Input setup parameters")
            (@arg PARAMS: +required "Input parameters json file")
            (@arg ZK_PROOF: +required "Output zero-knowledge proof")
        )
        (@subcommand verify =>
            (about: "Verify zero-knowledge proof")
            (@arg CONTRACT_DATA: +required "Input zero-knowledge contract data (.zcd)")
            (@arg SETUP_FILE: +required "Input setup parameters")
            (@arg ZK_PROOF: +required "Input zero-knowledge proof")
        )
        (@subcommand show =>
            (about: "Show public values in proof")
            (@arg ZK_PROOF: +required "Input zero-knowledge proof")
        )
    )
    .get_matches();

    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
    )
    .unwrap()])
    .unwrap();

    match matches.subcommand() {
        Some(("init", matches)) => {
            let contract_data: String = matches.value_of("CONTRACT_DATA").unwrap().parse()?;
            let setup_file: String = matches.value_of("SETUP_FILE").unwrap().parse()?;
            trusted_setup(contract_data, setup_file);
        }
        Some(("prove", matches)) => {
            let contract_data: String = matches.value_of("CONTRACT_DATA").unwrap().parse()?;
            let setup_file: String = matches.value_of("SETUP_FILE").unwrap().parse()?;
            let params: String = matches.value_of("PARAMS").unwrap().parse()?;
            let zk_proof: String = matches.value_of("ZK_PROOF").unwrap().parse()?;
            create_proof(contract_data, setup_file, params, zk_proof);
        }
        Some(("verify", matches)) => {
            let contract_data: String = matches.value_of("CONTRACT_DATA").unwrap().parse()?;
            let setup_file: String = matches.value_of("SETUP_FILE").unwrap().parse()?;
            let zk_proof: String = matches.value_of("ZK_PROOF").unwrap().parse()?;
            verify_proof(contract_data, setup_file, zk_proof);
        }
        Some(("show", matches)) => {
            let zk_proof: String = matches.value_of("ZK_PROOF").unwrap().parse()?;
            show_public(zk_proof);
        }
        _ => {
            eprintln!("error: Invalid subcommand invoked");
            std::process::exit(-1);
        }
    }

    Ok(())
}
