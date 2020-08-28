use bellman::groth16::*;
use bls12_381::Bls12;
use ff::{Field, PrimeField};
use group::Group;
use rand::{rngs::OsRng, seq::SliceRandom, CryptoRng};
use rand_core::{RngCore, SeedableRng};
use rand_xorshift::XorShiftRng;
use std::fs::File;
use std::time::{Duration, Instant};
use zcash_primitives::note_encryption::{Memo, SaplingNoteEncryption};
use zcash_primitives::primitives::{Diversifier, Note, ProofGenerationKey, Rseed, ValueCommitment};
use zcash_primitives::transaction::components::{Amount, GROTH_PROOF_SIZE};
use zcash_primitives::zip32::{ChildIndex, ExtendedFullViewingKey, ExtendedSpendingKey};
use zcash_primitives::sapling::{spend_sig, Node};
use zcash_primitives::redjubjub::PrivateKey;
use zcash_proofs::circuit::sapling::{Spend, Output};
use zcash_proofs::sapling::SaplingProvingContext;
use zcash_primitives::merkle_tree::{CommitmentTree, IncrementalWitness};

const TREE_DEPTH: usize = 32;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn generate_params() -> Result<()> {
    let mut rng = OsRng;

    println!("Creating spend parameters...");
    let start = Instant::now();
    let spend_params = generate_random_parameters::<Bls12, _, _>(
        Spend {
            value_commitment: None,
            proof_generation_key: None,
            payment_address: None,
            commitment_randomness: None,
            ar: None,
            auth_path: vec![None; TREE_DEPTH],
            anchor: None,
        },
        &mut rng,
    )
    .unwrap();
    let buffer = File::create("spend.params")?;
    spend_params.write(buffer)?;
    println!("Finished spend paramgen [{:?}]", start.elapsed());

    println!("Creating output parameters...");
    let start = Instant::now();
    let output_params = generate_random_parameters::<Bls12, _, _>(
        Output {
            value_commitment: None,
            payment_address: None,
            commitment_randomness: None,
            esk: None,
        },
        &mut rng,
    )
    .unwrap();
    let buffer = File::create("output.params")?;
    output_params.write(buffer)?;
    println!("Finished output paramgen [{:?}]", start.elapsed());

    Ok(())
}

fn main() -> Result<()> {
    //generate_params()?;

    let mut rng = OsRng;

    println!("Reading output parameters from file...");
    let start = Instant::now();
    let buffer = File::open("output.params")?;
    let output_params = Parameters::<Bls12>::read(buffer, false)?;
    let output_vk = prepare_verifying_key(&output_params.vk);
    println!("Finished load output params [{:?}]", start.elapsed());

    let mut ctx = SaplingProvingContext::new();

    let start = Instant::now();

    let seed = [0; 32];
    let xsk_m = ExtendedSpendingKey::master(&seed);
    //let xfvk_m = ExtendedFullViewingKey::from(&xsk_m);

    let i_5h = ChildIndex::Hardened(5);
    let secret_key = xsk_m.derive_child(i_5h);
    let viewing_key = ExtendedFullViewingKey::from(&secret_key);

    let (diversifier, payment_address) = viewing_key.default_address().unwrap();
    let ovk = viewing_key.fvk.ovk;

    let g_d = payment_address.g_d().expect("invalid address");
    let mut buffer = [0u8; 32];
    &rng.fill_bytes(&mut buffer);
    let rseed = Rseed::AfterZip212(buffer);

    let note = Note {
        g_d,
        pk_d: payment_address.pk_d().clone(),
        value: 10,
        rseed,
    };

    println!("Now we made the output [{:?}]", start.elapsed());
    // Ok(SaplingOutput {
    //     ovk,
    //     to,
    //     note,
    //     memo
    // })

    let start = Instant::now();

    let memo = Default::default();

    let encryptor =
        SaplingNoteEncryption::new(ovk, note.clone(), payment_address.clone(), memo, &mut rng);

    let esk = encryptor.esk().clone();
    let rcm = note.rcm();
    let value = note.value;
    let (proof, cv) = ctx.output_proof(esk, payment_address.clone(), rcm, value, &output_params);

    let mut zkproof = [0u8; GROTH_PROOF_SIZE];
    proof
        .write(&mut zkproof[..])
        .expect("should be able to serialize a proof");

    let cmu = note.cmu();

    let enc_ciphertext = encryptor.encrypt_note_plaintext();
    let out_ciphertext = encryptor.encrypt_outgoing_plaintext(&cv, &cmu);

    let ephemeral_key: jubjub::ExtendedPoint = encryptor.epk().clone().into();

    println!("Output description completed [{:?}]", start.elapsed());
    // OutputDescription {
    //     cv,
    //     cmu,
    //     ephemeral_key,
    //     enc_ciphertext,
    //     out_ciphertext,
    //     zkproof,
    // }

    println!("Reading spend parameters from file...");
    let start = Instant::now();
    let buffer = File::open("spend.params")?;
    let spend_params = Parameters::<Bls12>::read(buffer, false)?;
    let spend_vk = prepare_verifying_key(&spend_params.vk);
    println!("Finished spend paramgen [{:?}]", start.elapsed());

    let start = Instant::now();

    let cmu1 = Node::new(note.cmu().to_repr());
    let mut tree = CommitmentTree::new();
    tree.append(cmu1).unwrap();
    let witness = IncrementalWitness::from_tree(&tree);

    let alpha = jubjub::Fr::random(&mut rng);

    // Now we have the spend
    // SpendDescriptionInfo {
    //     extsk,
    //     diversifier,
    //     note,
    //     alpha,
    //     merkle_path,
    // }

    let proof_generation_key = secret_key.expsk.proof_generation_key();

    let merkle_path = witness.path().unwrap();

    let cmu = Node::new(note.cmu().into());
    let anchor = merkle_path.root(cmu).into();

    let mut nullifier = [0u8; 32];
    nullifier.copy_from_slice(&note.nf(
        &proof_generation_key.to_viewing_key(),
        merkle_path.position,
    ));

    let (proof, cv, rk) = ctx.spend_proof(
        proof_generation_key,
        payment_address.diversifier().clone(),
        rseed,
        alpha,
        value,
        anchor,
        merkle_path,
        &spend_params,
        &spend_vk,
    ).expect("Making proof failed");

    let mut zkproof = [0u8; GROTH_PROOF_SIZE];
    proof
        .write(&mut zkproof[..])
        .expect("should be able to serialize a proof");

    // Now we have a shielded spend
    // SpendDescription {
    //     cv,
    //     anchor,
    //     nullifier,
    //     rk,
    //     zkproof,
    //     spend_auth_sig: None,
    // }

    // Now for each spend in the tx, we create a signature
    // spendAuthSig
    // Signature of the entire transaction

    // Transaction hash into sighash. Just like in Bitcoin
    // Contains our SpendDescriptions and OutputDescriptions
    let mut sighash = [0u8; 32];
    let spend_auth_sig = spend_sig(
        PrivateKey(secret_key.expsk.ask),
        alpha,
        &sighash,
        &mut rng,
    );

    // And now use the sighash value (since it's signed by all inputs) to create a new key
    // which is used to sign the balance commitments.
    let amount = Amount::from_u64(0).unwrap();
    let binding_sig = ctx.binding_sig(amount, &sighash).expect("sighash binding sig failed");

    //let extsk = ExtendedSpendingKey::master(&[]);
    //let extfvk = ExtendedFullViewingKey::from(&extsk);
    //let to_address = extfvk.default_address().unwrap().1;
    // We will spend the address from above

    //pub extern "C" fn librustzcash_sapling_output_proof(
    //    ctx: *mut SaplingProvingContext,                  X
    //    esk: *const [c_uchar; 32],
    //    payment_address: *const [c_uchar; 43],
    //    rcm: *const [c_uchar; 32],
    //    value: u64,
    //    cv: *mut [c_uchar; 32],
    //    zkproof: *mut [c_uchar; GROTH_PROOF_SIZE],
    //) -> bool

    // Create proof
    //let (proof, value_commitment) = unsafe { &mut *ctx }.output_proof(
    //    esk,
    //    payment_address,
    //    rcm,
    //    value,
    //    unsafe { SAPLING_OUTPUT_PARAMS.as_ref() }.unwrap(),
    //);

    //// Write the proof out to the caller
    //proof
    //    .write(&mut (unsafe { &mut *zkproof })[..])
    //    .expect("should be able to serialize a proof");

    //// Write the value commitment to the caller
    //*unsafe { &mut *cv } = value_commitment.to_bytes();
    Ok(())
}
