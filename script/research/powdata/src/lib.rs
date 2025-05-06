use std::io::{self, Read, Write};

use darkfi_serial::{Decodable, Encodable};
use monero::{
    blockdata::transaction::RawExtraField,
    consensus::{Decodable as XmrDecodable, Encodable as XmrEncodable},
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
};
use tiny_keccak::{Hasher, Keccak};

mod error;

mod merkle_tree;
use merkle_tree::MerkleProof;

#[derive(Clone)]
pub struct MoneroPowData {
    /// Monero Header fields
    pub header: monero::BlockHeader,
    /// RandomX VM key - length varies to a max len of 60.
    /// TODO: Implement a type, or use randomx_key[0] to define len.
    pub randomx_key: [u8; 64],
    /// The number of transactions included in this Monero block.
    /// This is used to produce the blockhashing_blob.
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: monero::Hash,
    /// Coinbase Merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// Incomplete hashed state of the coinbase transaction
    pub coinbase_tx_hasher: Keccak,
    /// Extra field of the coinbase
    pub coinbase_tx_extra: RawExtraField,
    /// Aux chain Merkle proof hashes
    pub aux_chain_merkle_proof: MerkleProof,
}

impl Encodable for MoneroPowData {
    fn encode<S: Write>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        n += self.header.consensus_encode(s)?;
        n += self.randomx_key.encode(s)?;
        n += self.transaction_count.encode(s)?;
        n += self.merkle_root.as_fixed_bytes().encode(s)?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode(s)?;

        n += self.coinbase_tx_extra.0.encode(s)?;
        n += self.aux_chain_merkle_proof.encode(s)?;

        Ok(n)
    }
}

impl Decodable for MoneroPowData {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let header = monero::BlockHeader::consensus_decode(d)
            .map_err(|_| io::Error::other("Invalid XMR header"))?;

        let randomx_key: [u8; 64] = Decodable::decode(d)?;
        let transaction_count: u16 = Decodable::decode(d)?;

        let merkle_root: [u8; 32] = Decodable::decode(d)?;
        let merkle_root = monero::Hash::from_slice(&merkle_root);

        let coinbase_merkle_proof: MerkleProof = Decodable::decode(d)?;

        let buf: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf);

        let coinbase_tx_extra: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);

        let aux_chain_merkle_proof: MerkleProof = Decodable::decode(d)?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

impl MoneroPowData {
    /// Returns true if the coinbase Merkle proof produces the `merkle_root` hash.
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        let mut finalised_prefix_keccak = self.coinbase_tx_hasher.clone();
        let mut encoder_extra_field = vec![];
        self.coinbase_tx_extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let final_prefix_hash = monero::Hash::from_slice(&prefix_hash);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };

        let hashes = vec![final_prefix_hash, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> =
            hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase_hash = monero::Hash::new(encoder_final);

        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        (self.merkle_root == merkle_root) && self.coinbase_merkle_proof.check_coinbase_path()
    }
}

#[repr(C)]
enum Mode {
    Absorbing,
    Squeezing,
}

#[repr(C)]
// https://docs.rs/tiny-keccak/latest/src/tiny_keccak/lib.rs.html#368
struct KeccakState {
    buffer: [u8; 200],
    offset: usize,
    rate: usize,
    delim: u8,
    mode: Mode,
}

unsafe fn serialize_keccak<W: Write>(keccak: &Keccak, writer: &mut W) -> io::Result<()> {
    let keccak_ptr = keccak as *const Keccak as *const KeccakState;
    let keccak_state = &*keccak_ptr;

    writer.write_all(&keccak_state.buffer)?;
    writer.write_all(&(keccak_state.offset as u64).to_le_bytes())?;
    writer.write_all(&(keccak_state.rate as u64).to_le_bytes())?;
    writer.write_all(&[keccak_state.delim])?;

    Ok(())
}

unsafe fn deserialize_keccak<R: Read>(reader: &mut R) -> io::Result<Keccak> {
    let mut keccak = Keccak::v256();

    let keccak_ptr = &mut keccak as *mut Keccak as *mut KeccakState;
    let keccak_state = &mut *keccak_ptr;

    reader.read_exact(&mut keccak_state.buffer)?;

    let mut offset_bytes = [0u8; 8];
    reader.read_exact(&mut offset_bytes)?;
    keccak_state.offset = u64::from_le_bytes(offset_bytes) as usize;

    let mut rate_bytes = [0u8; 8];
    reader.read_exact(&mut rate_bytes)?;
    keccak_state.rate = u64::from_le_bytes(rate_bytes) as usize;

    let mut delim_byte = [0u8; 1];
    reader.read_exact(&mut delim_byte)?;
    keccak_state.delim = delim_byte[0];

    keccak_state.mode = Mode::Absorbing;

    Ok(keccak)
}

fn keccak_to_bytes(keccak: &Keccak) -> Vec<u8> {
    let mut bytes = vec![];
    unsafe { serialize_keccak(keccak, &mut bytes).unwrap() }
    bytes
}

fn keccak_from_bytes(bytes: &[u8]) -> Keccak {
    let mut cursor = io::Cursor::new(bytes);
    unsafe { deserialize_keccak(&mut cursor).unwrap() }
}

#[test]
fn test_keccak_serde() {
    let mut keccak = Keccak::v256();
    keccak.update(b"foobar");

    let ser = keccak_to_bytes(&keccak);

    let mut digest1 = [0u8; 32];
    keccak.finalize(&mut digest1);

    let de = keccak_from_bytes(&ser);
    let mut digest2 = [0u8; 32];
    de.finalize(&mut digest2);

    println!("{:?}", digest1);
    println!("{:?}", digest2);

    assert_eq!(digest1, digest2);
}
