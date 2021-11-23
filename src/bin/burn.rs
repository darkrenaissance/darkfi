use std::iter;
use std::time::Instant;

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
        FixedPoint, FixedPoints,
    },
    poseidon::{
        Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig,
        StateWord, Word,
    },
    primitives,
    primitives::{
        poseidon::{ConstantLength, P128Pow5T3},
        sinsemilla::S_PERSONALIZATION,
    },
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::chip::{MerkleChip, MerkleConfig},
        merkle::MerklePath,
    },
    utilities::{
        copy, lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions, Var,
    },
};
use incrementalmerkletree::{
    bridgetree::{BridgeTree, Frontier as BridgeFrontier},
    Altitude, Frontier, Tree,
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{
        ff::{PrimeField, PrimeFieldBits},
        Curve,
    },
    pallas,
};
use rand::rngs::OsRng;

use drk::crypto::{
    constants::{
        sinsemilla::{
            i2lebsp, OrchardCommitDomains, OrchardHashDomains, MERKLE_CRH_PERSONALIZATION,
        },
        OrchardFixedBases,
    },
    merkle_node2::MerkleNode,
    proof::{Proof, ProvingKey, VerifyingKey},
    util::{pedersen_commitment_scalar, pedersen_commitment_u64},
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

    /*
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
    */

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

// The public input array offsets
const BURN_NULLIFIER_OFFSET: usize = 0;
const BURN_VALCOMX_OFFSET: usize = 1;
const BURN_VALCOMY_OFFSET: usize = 2;
const BURN_ASSCOMX_OFFSET: usize = 3;
const BURN_ASSCOMY_OFFSET: usize = 4;
const BURN_MERKLEROOT_OFFSET: usize = 5;
const BURN_SIGKEYX_OFFSET: usize = 6;
const BURN_SIGKEYY_OFFSET: usize = 7;

#[derive(Default, Debug)]
struct BurnCircuit {
    secret_key: Option<pallas::Base>,
    serial: Option<pallas::Base>,
    value: Option<pallas::Base>,
    asset: Option<pallas::Base>,
    coin_blind: Option<pallas::Base>,
    value_blind: Option<pallas::Scalar>,
    asset_blind: Option<pallas::Scalar>,
    leaf: Option<pallas::Base>,
    leaf_pos: Option<u32>,
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

        // Construct the merkle chips
        let merkle_chip_1 = config.merkle_chip_1();
        let merkle_chip_2 = config.merkle_chip_2();

        // =========
        // Nullifier
        // =========
        let secret_key = self.load_private(
            layouter.namespace(|| "load sinsemilla(secret key)"),
            config.advices[0],
            self.secret_key,
        )?;

        let serial = self.load_private(
            layouter.namespace(|| "load serial"),
            config.advices[0],
            self.serial,
        )?;

        let message = [secret_key, serial];
        let hash = {
            let poseidon_message = layouter.assign_region(
                || "load message",
                |mut region| {
                    let mut message_word = |i: usize| {
                        let value = message[i].value();
                        let var = region.assign_advice(
                            || format!("load message_{}", i),
                            config.poseidon_config.state()[i],
                            0,
                            || value.ok_or(Error::SynthesisError),
                        )?;
                        region.constrain_equal(var, message[i].cell())?;
                        Ok(Word::<_, _, P128Pow5T3, 3, 2>::from_inner(StateWord::new(
                            var, value,
                        )))
                    };
                    Ok([message_word(0)?, message_word(1)?])
                },
            )?;

            let poseidon_hasher = PoseidonHash::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
                ConstantLength::<2>,
            )?;

            let poseidon_output = poseidon_hasher.hash(
                layouter.namespace(|| "Poseidon hash (secretkey, serial)"),
                poseidon_message,
            )?;

            let poseidon_output: CellValue<pallas::Base> = poseidon_output.inner().into();
            poseidon_output
        };

        layouter.constrain_instance(hash.cell(), config.primary, BURN_NULLIFIER_OFFSET)?;

        // let nullifier_k = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
        //     nullifier_k.mul(
        //         layouter.namespace(|| "[poseidon_output + psi_old] NullifierK"),
        //         scalar,
        //     )?

        let value = self.load_private(
            layouter.namespace(|| "load value"),
            config.advices[0],
            self.value,
        )?;

        let asset = self.load_private(
            layouter.namespace(|| "load asset"),
            config.advices[0],
            self.asset,
        )?;

        let coin_blind = self.load_private(
            layouter.namespace(|| "load coin_blind"),
            config.advices[0],
            self.coin_blind,
        )?;

        let (public_key, _) = {
            let spend_auth_g = OrchardFixedBases::SpendAuthG;
            let spend_auth_g = FixedPoint::from_inner(ecc_chip, spend_auth_g);
            // TODO: Do we need to load sig_secret somewhere first?
            spend_auth_g.mul(layouter.namespace(|| "[x_s] SpendAuthG"), self.secret_key)?
        };

        let (pub_x, pub_y) = (public_key.inner().x().cell(), public_key.inner().y().cell());

        // =========
        // Coin hash
        // =========
        let messages = [[pub_x, pub_y], [value, asset], [serial, coin_blind]];
        let mut hashes = vec![];

        for message in messages.iter() {
            let hash = {
                let poseidon_message = layouter.assign_region(
                    || "load message",
                    |mut region| {
                        let mut message_word = |i: usize| {
                            let value = message[i].value();
                            let var = region.assign_advice(
                                || format!("load message_{}", i),
                                config.poseidon_config.state()[i],
                                0,
                                || value.ok_or(Error::SynthesisError),
                            )?;
                            region.constrain_equal(var, message[i].cell())?;
                            Ok(Word::<_, _, P128Pow5T3, 3, 2>::from_inner(StateWord::new(
                                var, value,
                            )))
                        };
                        Ok([message_word(0)?, message_word(1)?])
                    },
                )?;

                let poseidon_hasher = PoseidonHash::init(
                    config.poseidon_chip(),
                    layouter.namespace(|| "Poseidon init"),
                    ConstantLength::<2>,
                )?;

                let poseidon_output = poseidon_hasher.hash(
                    layouter.namespace(|| "Poseidon hash (a, b)"),
                    poseidon_message,
                )?;

                let poseidon_output: CellValue<pallas::Base> = poseidon_output.inner().into();
                poseidon_output
            };

            hashes.push(hash);
        }

        let coin = layouter.assign_region(
            || " `coin` = hash(a,b) + hash(c, d) + hash(e, f)",
            |mut region| {
                config.q_add.enable(&mut region, 0)?;

                copy(&mut region, || "copy ab", config.advices[6], 0, &hashes[0])?;
                copy(&mut region, || "copy cd", config.advices[7], 0, &hashes[1])?;
                copy(&mut region, || "copy ef", config.advices[8], 0, &hashes[2])?;

                let scalar_val = hashes[0]
                    .value()
                    .zip(hashes[1].value())
                    .zip(hashes[2].value())
                    .map(|(abcd, ef)| abcd.0 + abcd.1 + ef);

                let cell = region.assign_advice(
                    || "hash(a,b)+hash(c,d)+hash(e,f)",
                    config.advices[5],
                    0,
                    || scalar_val.ok_or(Error::SynthesisError),
                )?;
                Ok(CellValue::new(cell, scalar_val))
            },
        )?;

        // ===========
        // Merkle root
        // ===========
        let leaf = self.load_private(
            layouter.namespace(|| "load leaf"),
            config.advices[0],
            self.leaf,
        )?;

        let path = MerklePath {
            chip_1: merkle_chip_1,
            chip_2: merkle_chip_2,
            domain: OrchardHashDomains::MerkleCrh,
            leaf_pos: self.leaf_pos,
            path: self.merkle_path,
        };

        let computed_final_root =
            path.calculate_root(layouter.namespace(|| "calculate root"), leaf)?;

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            BURN_MERKLEROOT_OFFSET,
        )?;

        // ================
        // Value commitment
        // ================

        // This constant one is used for multiplication
        let one = self.load_private(
            layouter.namespace(|| "load constant one"),
            config.advices[0],
            Some(pallas::Base::one()),
        )?;

        let value = self.load_private(
            layouter.namespace(|| "load value"),
            config.advices[0],
            self.value,
        )?;

        // v * G_1
        let (commitment, _) = {
            let value_commit_v = OrchardFixedBases::ValueCommitV;
            let value_commit_v = FixedPoint::from_inner(ecc_chip.clone(), value_commit_v);
            value_commit_v.mul_short(layouter.namespace(|| "[value] ValueCommitV"), (value, one))?
        };

        // r_V * G_2
        let (blind, _rcv) = {
            let rcv = self.value_blind;
            let value_commit_r = OrchardFixedBases::ValueCommitR;
            let value_commit_r = FixedPoint::from_inner(ecc_chip.clone(), value_commit_r);
            value_commit_r.mul(layouter.namespace(|| "[value_blind] ValueCommitR"), rcv)?
        };

        // Constrain the value commitment coordinates
        let value_commit = commitment.add(layouter.namespace(|| "valuecommit"), &blind)?;
        layouter.constrain_instance(
            value_commit.inner().x().cell(),
            config.primary,
            BURN_VALCOMX_OFFSET,
        )?;
        layouter.constrain_instance(
            value_commit.inner().y().cell(),
            config.primary,
            BURN_VALCOMY_OFFSET,
        )?;

        // ================
        // Asset commitment
        // ================

        let asset = self.load_private(
            layouter.namespace(|| "load asset"),
            config.advices[0],
            self.asset,
        )?;

        // a * G_1
        let (commitment, _) = {
            let asset_commit_v = OrchardFixedBases::ValueCommitV;
            let asset_commit_v = FixedPoint::from_inner(ecc_chip.clone(), asset_commit_v);
            asset_commit_v.mul_short(layouter.namespace(|| "[asset] ValueCommitV"), (asset, one))?
        };

        // r_A * G_2
        let (blind, _rca) = {
            let rca = self.asset_blind;
            let asset_commit_r = OrchardFixedBases::ValueCommitR;
            let asset_commit_r = FixedPoint::from_inner(ecc_chip.clone(), asset_commit_r);
            asset_commit_r.mul(layouter.namespace(|| "[asset_blind] ValueCommitR"), rca)?
        };

        // Constrain the asset commitment coordinates
        let asset_commit = commitment.add(layouter.namespace(|| "assetcommit"), &blind)?;
        layouter.constrain_instance(
            asset_commit.inner().x().cell(),
            config.primary,
            BURN_ASSCOMX_OFFSET,
        )?;
        layouter.constrain_instance(
            asset_commit.inner().y().cell(),
            config.primary,
            BURN_ASSCOMY_OFFSET,
        )?;

        // ========================
        // Signature key derivation
        // ========================
        let (sig_pub, _) = {
            let spend_auth_g = OrchardFixedBases::SpendAuthG;
            let spend_auth_g = FixedPoint::from_inner(ecc_chip, spend_auth_g);
            // TODO: Do we need to load sig_secret somewhere first?
            spend_auth_g.mul(layouter.namespace(|| "[x_s] SpendAuthG"), self.sig_secret)?
        };

        layouter.constrain_instance(
            sig_pub.inner().x().cell(),
            config.primary,
            BURN_SIGKEYX_OFFSET,
        )?;
        layouter.constrain_instance(
            sig_pub.inner().y().cell(),
            config.primary,
            BURN_SIGKEYY_OFFSET,
        )?;

        // At this point we've enforced all of our public inputs.
        Ok(())
    }
}

fn root(path: [pallas::Base; 32], leaf_pos: u32, leaf: pallas::Base) -> pallas::Base {
    let domain = primitives::sinsemilla::HashDomain::new(MERKLE_CRH_PERSONALIZATION);

    let pos_bool = i2lebsp::<32>(leaf_pos as u64);

    let mut node = leaf;
    for (l, (sibling, pos)) in path.iter().zip(pos_bool.iter()).enumerate() {
        let (left, right) = if *pos {
            (*sibling, node)
        } else {
            (node, *sibling)
        };

        let l_star = i2lebsp::<10>(l as u64);
        let left: Vec<_> = left.to_le_bits().iter().by_val().take(255).collect();
        let right: Vec<_> = right.to_le_bits().iter().by_val().take(255).collect();

        let mut message = l_star.to_vec();
        message.extend_from_slice(&left);
        message.extend_from_slice(&right);

        node = domain.hash(message.into_iter()).unwrap();
    }
    node
}

fn mod_r_p(x: pallas::Base) -> pallas::Scalar {
    pallas::Scalar::from_repr(x.to_repr()).unwrap()
}

fn main() {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 11;

    let secret = pallas::Base::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);

    let value = 42;
    let asset = 1;

    // Nullifier = poseidon(sinsemilla(secret_key), serial)
    let domain = primitives::sinsemilla::HashDomain::new(S_PERSONALIZATION);
    //let bits_secretkey: Vec<bool> = secret_key.to_le_bits().iter().by_val().collect();
    //let hashed_secret_key = domain.hash(iter::empty().chain(bits_secretkey)).unwrap();

    let nullifier = [secret, serial];
    let nullifier =
        primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(nullifier);

    // Public key derivation
    let public_key = OrchardFixedBases::SpendAuthG.generator() * mod_r_p(secret);
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

    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    for _ in 0..10 {
        let random_node = MerkleNode(pallas::Base::random(&mut OsRng));
        tree.append(&random_node);
    }
    let node = MerkleNode(coin.clone());
    tree.append(&node);
    tree.witness();
    for _ in 0..10 {
        let random_node = MerkleNode(pallas::Base::random(&mut OsRng));
        tree.append(&random_node);
    }

    let (merkle_position, merkle_path) = tree.authentication_path(&node).unwrap();

    // Merkle root
    //let leaf = pallas::Base::random(&mut OsRng);
    let leaf = coin.clone();
    let pos: u64 = merkle_position.into();
    let path: Vec<pallas::Base> = merkle_path.iter().map(|node| node.0).collect();
    let merkle_root = tree.root().0;

    // Value and asset commitments
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let asset_blind = pallas::Scalar::random(&mut OsRng);
    let value_commit = pedersen_commitment_u64(value, value_blind);
    let asset_commit = pedersen_commitment_u64(asset, asset_blind);

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
        secret_key: Some(secret),
        serial: Some(serial),
        value: Some(pallas::Base::from(value)),
        asset: Some(pallas::Base::from(asset)),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(asset_blind),
        leaf: Some(leaf),
        leaf_pos: Some(pos as u32),
        merkle_path: Some(path.try_into().unwrap()),
        sig_secret: Some(sig_secret),
    };

    let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    // Actual ZK proof
    let start = Instant::now();
    let vk = VerifyingKey::build(k, BurnCircuit::default());
    let pk = ProvingKey::build(k, BurnCircuit::default());
    println!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
    println!("Verify: [{:?}]", start.elapsed());
}
