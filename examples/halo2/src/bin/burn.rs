use std::iter;

use halo2::{
    circuit::{Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk::{
        Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn, Selector,
    },
    poly::Rotation,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoints,
    },
    poseidon::{Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig},
    primitives,
    primitives::{
        poseidon::{ConstantLength, P128Pow5T3},
        sinsemilla::S_PERSONALIZATION,
    },
    sinsemilla,
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::chip::{MerkleChip, MerkleConfig},
        merkle::MerklePath,
    },
    utilities::{
        gen_const_array, lookup_range_check::LookupRangeCheckConfig, CellValue,
        UtilitiesInstructions, Var,
    },
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{ff::PrimeFieldBits, Curve},
    pallas,
};
use rand::rngs::OsRng;

use drk_halo2::{
    constants::sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
    constants::OrchardFixedBases,
    crypto::pedersen_commitment,
};

#[derive(Clone, Debug)]
struct BurnConfig {
    primary: Column<InstanceColumn>,
    q_add: Selector,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base>,
}

impl BurnConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    fn sinsemilla_chip_1(
        &self,
    ) -> SinsemillaChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        SinsemillaChip::construct(self.sinsemilla_config_1.clone())
    }

    fn sinsemilla_chip_2(
        &self,
    ) -> SinsemillaChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        SinsemillaChip::construct(self.sinsemilla_config_2.clone())
    }

    fn merkle_chip_1(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_config_1.clone())
    }

    fn merkle_chip_2(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_config_2.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

#[derive(Default, Debug)]
struct BurnCircuit {
    secret_key: Option<pallas::Scalar>,
    serial: Option<pallas::Base>,
    value: Option<pallas::Base>,
    asset: Option<pallas::Base>,
    coin_blind: Option<pallas::Base>,
    value_blind: Option<pallas::Scalar>,
    asset_blind: Option<pallas::Scalar>,
    //merkle_path: Option<Vec<(pallas::Base, bool)>>,
    merkle_path: Option<[pallas::Base; 32]>,
    sig_secret: Option<pallas::Scalar>,
}

impl UtilitiesInstructions<pallas::Base> for BurnCircuit {
    type Var = CellValue<pallas::Base>;
}

impl Circuit<pallas::Base> for BurnCircuit {
    type Config = BurnConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        // Advice columns used in the circuit
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        // Addition of three field elements
        let q_add = meta.selector();
        meta.create_gate("a+b+c", |meta| {
            let q_add = meta.query_selector(q_add);
            let sum = meta.query_advice(advices[5], Rotation::cur());
            let a = meta.query_advice(advices[6], Rotation::cur());
            let b = meta.query_advice(advices[7], Rotation::cur());
            let c = meta.query_advice(advices[8], Rotation::cur());

            vec![q_add * (a + b + c - sum)]
        });

        // Fixed columns for the Sinsemilla generator lookup table
        let table_idx = meta.lookup_table_column();
        let lookup = (
            table_idx,
            meta.lookup_table_column(),
            meta.lookup_table_column(),
        );

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary.into());

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality((*advice).into());
        }

        // Poseidon requires four advice columns, while ECC incomplete addition
        // requires six. We can reduce the proof size by sharing fixed columns
        // between the ECC and Poseidon chips.
        // TODO: For multiple invocations they could/should be configured in
        // parallel rather than sharing perhaps?
        let lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];
        let rc_a = lagrange_coeffs[2..5].try_into().unwrap();
        let rc_b = lagrange_coeffs[5..8].try_into().unwrap();

        // Also use the first Lagrange coefficient column for loading global constants.
        meta.enable_constant(lagrange_coeffs[0]);

        // Use one of the right-most advice columns for all of our range checks.
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        // Configuration for curve point operations.
        // This uses 10 advice columns and spans the whole circuit.
        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices,
            lagrange_coeffs,
            range_check.clone(),
        );

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure(
            meta,
            P128Pow5T3,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_config_1, merkle_config_1) = {
            let sinsemilla_config_1 = SinsemillaChip::configure(
                meta,
                advices[..5].try_into().unwrap(),
                advices[6],
                lagrange_coeffs[0],
                lookup,
                range_check.clone(),
            );
            let merkle_config_1 = MerkleChip::configure(meta, sinsemilla_config_1.clone());
            (sinsemilla_config_1, merkle_config_1)
        };

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_config_2, merkle_config_2) = {
            let sinsemilla_config_2 = SinsemillaChip::configure(
                meta,
                advices[5..].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2.clone());

            (sinsemilla_config_2, merkle_config_2)
        };

        BurnConfig {
            primary,
            q_add,
            advices,
            ecc_config,
            merkle_config_1,
            merkle_config_2,
            sinsemilla_config_1,
            sinsemilla_config_2,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;

        // Construct the ECC chip.
        let ecc_chip = config.ecc_chip();

        /*
        // Merkle path validity check
        let anchor = {
            let path = self.merkle_path.map(|typed_path| {
                // TODO: Replace with array::map once MSRV is 1.55.0.
                gen_const_array(|i| typed_path[i].inner())
            });
            let merkle_inputs = MerklePath {
                chip_1: config.merkle_chip_1(),
                chip_2: config.merkle_chip_2(),
                domain: OrchardHashDomains::MerkleCrh,
                leaf_pos: self.pos,
                path,
            };
            let leaf = *cm_old.extract_p().inner();
            merkle_inputs.calculate_root(layouter.namespace(|| "MerkleCRH"), leaf)?
        };

        // Enforce the merkle root
        layouter.constrain_instance(anchor.cell(), config.primary, 5)?;
        */

        Ok(())
    }
}

fn main() {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 11;

    let secret_key = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);

    let value = 42;
    let asset = 1;

    // Nullifier = SinsemillaHash(secret_key, serial)
    let domain = primitives::sinsemilla::HashDomain::new(S_PERSONALIZATION);
    let bits_secretkey: Vec<bool> = secret_key.to_le_bits().iter().by_val().collect();
    let bits_serial: Vec<bool> = serial.to_le_bits().iter().by_val().collect();
    let nullifier = domain
        .hash(iter::empty().chain(bits_secretkey).chain(bits_serial))
        .unwrap();

    // Public key derivation
    let public_key = OrchardFixedBases::SpendAuthG.generator() * secret_key;
    let coords = public_key.to_affine().coordinates().unwrap();

    // Construct Coin
    let mut coin = pallas::Base::zero();
    let coin_blind = pallas::Base::random(&mut OsRng);
    let messages = [
        [*coords.x(), *coords.y()],
        [pallas::Base::from(value), pallas::Base::from(asset)],
        [serial, coin_blind],
    ];

    for msg in messages.iter() {
        let hash = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(*msg);
        coin += hash;
    }

    // Merkle root
    let merkle_root = pallas::Base::random(&mut OsRng);

    // Value and asset commitments
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let asset_blind = pallas::Scalar::random(&mut OsRng);
    let value_commit = pedersen_commitment(value, value_blind);
    let asset_commit = pedersen_commitment(asset, asset_blind);

    let value_coords = value_commit.to_affine().coordinates().unwrap();
    let asset_coords = asset_commit.to_affine().coordinates().unwrap();

    // Derive signature public key from signature secret key
    let sig_secret = pallas::Scalar::random(&mut OsRng);
    let sig_pubkey = OrchardFixedBases::SpendAuthG.generator() * sig_secret;
    let sig_coords = sig_pubkey.to_affine().coordinates().unwrap();

    let public_inputs = vec![
        nullifier,
        *value_coords.x(),
        *value_coords.y(),
        *asset_coords.x(),
        *asset_coords.y(),
        merkle_root,
        *sig_coords.x(),
        *sig_coords.y(),
    ];

    let circuit = BurnCircuit {
        secret_key: Some(secret_key),
        serial: Some(serial),
        value: Some(pallas::Base::from(value)),
        asset: Some(pallas::Base::from(asset)),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(asset_blind),
        merkle_path: None,
        sig_secret: Some(sig_secret),
    };

    let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
