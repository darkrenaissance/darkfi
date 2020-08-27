use bellman::groth16::*;
use bls12_381::Bls12;
use ff::Field;
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
use zcash_proofs::circuit::sapling::Spend;
use zcash_proofs::sapling::SaplingProvingContext;

const TREE_DEPTH: usize = 32;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let rng = &mut XorShiftRng::from_seed([
        0x59, 0x62, 0xbe, 0x3d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc,
        0xe5,
    ]);

    //println!("Creating sample parameters...");
    //let start = Instant::now();
    //let groth_params = generate_random_parameters::<Bls12, _, _>(
    //    Spend {
    //        value_commitment: None,
    //        proof_generation_key: None,
    //        payment_address: None,
    //        commitment_randomness: None,
    //        ar: None,
    //        auth_path: vec![None; TREE_DEPTH],
    //        anchor: None,
    //    },
    //    rng,
    //)
    //.unwrap();
    //let buffer = File::create("foo.txt")?;
    //groth_params.write(buffer)?;
    //println!("Finished paramgen [{:?}]", start.elapsed());

    println!("Reading parameters from file...");
    let start = Instant::now();
    let buffer = File::open("foo.txt")?;
    let groth_params = Parameters::<Bls12>::read(buffer, false)?;
    println!("Finished paramgen [{:?}]", start.elapsed());

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
    let mut rng = OsRng;
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
    let (proof, cv) = ctx.output_proof(esk, payment_address, rcm, value, &groth_params);

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
