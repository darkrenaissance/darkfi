// This tutorial example corresponds to the VM proof in proofs/tutorial.psm
// It encodes the same function as the one in zk-explainer document.
use bls12_381::Scalar;
use drk::{BlsStringConversion, Decodable, Encodable, ZKContract, ZKProof};
use std::fs::File;
use std::time::Instant;

type Result<T> = std::result::Result<T, failure::Error>;

fn main() -> Result<()> {
    {
        // Load the contract from file

        let start = Instant::now();
        let file = File::open("tutorial.zcd")?;
        let mut contract = ZKContract::decode(file)?;
        println!(
            "Loaded contract '{}': [{:?}]",
            contract.name,
            start.elapsed()
        );

        println!("Stats:");
        println!("    Constants: {}", contract.vm.constants.len());
        println!("    Alloc: {}", contract.vm.alloc.len());
        println!("    Operations: {}", contract.vm.ops.len());
        println!(
            "    Constraint Instructions: {}",
            contract.vm.constraints.len()
        );

        // Do the trusted setup

        contract.setup("tutorial.zts")?;
    }

    // Load the contract from file

    let start = Instant::now();
    let file = File::open("tutorial.zcd")?;
    let mut contract = ZKContract::decode(file)?;
    println!(
        "Loaded contract '{}': [{:?}]",
        contract.name,
        start.elapsed()
    );

    contract.load_setup("tutorial.zts")?;

    {
        // Put in our input parameters

        contract.set_param(
            "w",
            Scalar::from_string("0000000000000000000000000000000000000000000000000000000000000001"),
        )?;
        contract.set_param(
            "a",
            Scalar::from_string("0000000000000000000000000000000000000000000000000000000000000001"),
        )?;
        contract.set_param(
            "b",
            Scalar::from_string("0000000000000000000000000000000000000000000000000000000000000004"),
        )?;

        // Generate the ZK proof

        let proof = contract.prove()?;

        // Test and show our output values

        assert_eq!(proof.public.len(), 1);
        println!("v = {:?}", proof.public.get("v").unwrap());

        let mut file = File::create("tutorial.prf")?;
        proof.encode(&mut file)?;
    }

    // Verify the proof

    let file = File::open("tutorial.prf")?;
    let proof = ZKProof::decode(file)?;
    assert!(contract.verify(&proof));

    Ok(())
}
