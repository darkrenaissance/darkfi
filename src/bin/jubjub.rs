use bls12_381::Scalar;
use sapvi::{BlsStringConversion, Decodable, ZKContract};
use std::fs::File;
use std::time::Instant;

type Result<T> = std::result::Result<T, failure::Error>;

fn main() -> Result<()> {
    {
        // Load the contract from file

        let start = Instant::now();
        let file = File::open("jubjub.zcd")?;
        let mut contract = ZKContract::decode(file)?;
        println!("Loaded contract '{}': [{:?}]", contract.name, start.elapsed());

        println!("Stats:");
        println!("    Constants: {}", contract.vm.constants.len());
        println!("    Alloc: {}", contract.vm.alloc.len());
        println!("    Operations: {}", contract.vm.ops.len());
        println!(
            "    Constraint Instructions: {}",
            contract.vm.constraints.len()
        );

        // Do the trusted setup

        contract.setup("jubjub.zts")?;
    }

    // Load the contract from file

    let start = Instant::now();
    let file = File::open("jubjub.zcd")?;
    let mut contract = ZKContract::decode(file)?;
    println!("Loaded contract '{}': [{:?}]", contract.name, start.elapsed());

    contract.load_setup("jubjub.zts")?;

    // Put in our input parameters

    contract.set_param(
        "a_u",
        Scalar::from_string("15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"),
    )?;
    contract.set_param(
        "a_v",
        Scalar::from_string("015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"),
    )?;
    contract.set_param(
        "b_u",
        Scalar::from_string("15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"),
    )?;
    contract.set_param(
        "b_v",
        Scalar::from_string("015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"),
    )?;

    // Generate the ZK proof

    let proof = contract.prove()?;

    // Test and show our output values

    assert_eq!(proof.public.len(), 2);
    // 0x66ced46f14e5616d12b993f60a6e66558d6b6afe4c321ed212e0b9cfbd81061a
    assert_eq!(
        *proof.public.get("result_u").unwrap(),
        Scalar::from_string("66ced46f14e5616d12b993f60a6e66558d6b6afe4c321ed212e0b9cfbd81061a")
    );
    // 0x4731570fdd57cf280eadc8946fa00df81112502e44e497e794ab9a221f1bcca
    assert_eq!(
        *proof.public.get("result_v").unwrap(),
        Scalar::from_string("04731570fdd57cf280eadc8946fa00df81112502e44e497e794ab9a221f1bcca")
    );
    println!("u = {:?}", proof.public.get("result_u").unwrap());
    println!("v = {:?}", proof.public.get("result_v").unwrap());

    // Verify the proof

    assert!(contract.verify(&proof));

    Ok(())
}
