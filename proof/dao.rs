use bitvec::prelude::*;
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointShort, Point,
    },
    poseidon::{Hash as PoseidonHash, Pow5Chip as PoseidonChip, Pow5Config as PoseidonConfig},
    primitives::{
        poseidon,
        poseidon::{ConstantLength, P128Pow5T3},
    },
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::{
            chip::{MerkleChip, MerkleConfig},
            MerklePath,
        },
    },
    utilities::{lookup_range_check::LookupRangeCheckConfig, UtilitiesInstructions},
};
use halo2_proofs::{
    arithmetic::Field,
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use log::debug;
use pasta_curves::{
    arithmetic::{CurveAffine, FieldExt},
    group::{ff::PrimeField, Curve, Group},
    pallas,
};
use rand::rngs::OsRng;
use simplelog::{ColorChoice::Auto, Config, LevelFilter, TermLogger, TerminalMode::Mixed};

use darkfi::{
    crypto::{
        constants::{
            sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
            util::gen_const_array,
            OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
        },
        keypair::Keypair,
        merkle_node::MerkleNode,
        schnorr::SchnorrSecret,
        util::{mod_r_p, pedersen_commitment_scalar},
    },
    Result,
};

#[derive(Clone)]
pub struct VmConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    merkle_cfg1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_cfg2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_cfg1: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    _sinsemilla_cfg2: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
}

impl VmConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    fn merkle_chip_1(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_cfg1.clone())
    }

    fn merkle_chip_2(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_cfg2.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

#[derive(Clone, Default)]
pub struct ZkCircuit {
    a: Option<pallas::Base>,   // contract address
    s: Option<pallas::Base>,   // serial number
    t: Option<pallas::Base>,   // treasury balance
    b_b: Option<pallas::Base>, // bulla blinding

    leaf_pos: Option<u32>,
    merkle_path: Option<[MerkleNode; 32]>,

    u: Option<pallas::Base>,   // output 0 value
    p_x: Option<pallas::Base>, // output0 pub_x
    p_y: Option<pallas::Base>, // output0 pub_y
    b_m: Option<pallas::Base>, // output0 blind

    votes: Option<pallas::Base>,
    vote_blinds: Option<pallas::Scalar>,

    output_1_blind: Option<pallas::Scalar>,
}

impl UtilitiesInstructions<pallas::Base> for ZkCircuit {
    type Var = AssignedCell<pallas::Base, pallas::Base>;
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = VmConfig;
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

        // Fixed columns for the Sinsemilla generator lookup table
        let table_idx = meta.lookup_table_column();
        let lookup = (table_idx, meta.lookup_table_column(), meta.lookup_table_column());

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary);

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        // Poseidon requires four advice columns, while ECC incomplete addition
        // requires six. We can reduce the proof size by sharing fixed columns
        // between the ECC and Poseidon chips.
        // TODO: For multiple invocations perhaps they could/should be configured
        // in parallel rather than sharing?
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
        let poseidon_config = PoseidonChip::configure::<P128Pow5T3>(
            meta,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_cfg1, merkle_cfg1) = {
            let sinsemilla_cfg1 = SinsemillaChip::configure(
                meta,
                advices[..5].try_into().unwrap(),
                advices[6],
                lagrange_coeffs[0],
                lookup,
                range_check.clone(),
            );
            let merkle_cfg1 = MerkleChip::configure(meta, sinsemilla_cfg1.clone());
            (sinsemilla_cfg1, merkle_cfg1)
        };

        let (_sinsemilla_cfg2, merkle_cfg2) = {
            let sinsemilla_cfg2 = SinsemillaChip::configure(
                meta,
                advices[5..].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_cfg2 = MerkleChip::configure(meta, sinsemilla_cfg2.clone());
            (sinsemilla_cfg2, merkle_cfg2)
        };

        VmConfig {
            primary,
            advices,
            ecc_config,
            merkle_cfg1,
            merkle_cfg2,
            sinsemilla_cfg1,
            _sinsemilla_cfg2,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> std::result::Result<(), plonk::Error> {
        debug!("Entering synthesize()");
        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_cfg1.clone(), &mut layouter)?;

        // Construct the ECC chip.
        let ecc_chip = config.ecc_chip();

        // This constant one is used for short multiplication
        let one = self.load_private(
            layouter.namespace(|| "Load constant one"),
            config.advices[0],
            Some(pallas::Base::one()),
        )?;

        let contract_address = self.load_private(
            layouter.namespace(|| "Load contract address"),
            config.advices[0],
            self.a,
        )?;

        let serial_number = self.load_private(
            layouter.namespace(|| "Load serial number"),
            config.advices[0],
            self.s,
        )?;

        let treasury_balance = self.load_private(
            layouter.namespace(|| "Load treasury balance"),
            config.advices[0],
            self.t,
        )?;

        let bulla_blind = self.load_private(
            layouter.namespace(|| "Load bulla blind"),
            config.advices[0],
            self.b_b,
        )?;

        let output0_value = self.load_private(
            layouter.namespace(|| "Load output0 value"),
            config.advices[0],
            self.u,
        )?;

        let output0_pub_x = self.load_private(
            layouter.namespace(|| "Load output0 dest pub x"),
            config.advices[0],
            self.p_x,
        )?;

        let output0_pub_y = self.load_private(
            layouter.namespace(|| "Load output0 dest pub y"),
            config.advices[0],
            self.p_y,
        )?;

        let output0_blind = self.load_private(
            layouter.namespace(|| "Load output0 blind"),
            config.advices[0],
            self.b_m,
        )?;

        let votes = self.load_private(
            layouter.namespace(|| "Load votes summed"),
            config.advices[0],
            self.votes,
        )?;

        // Constrain the serial number
        println!("Serial in circuit: {:?}", serial_number.value());
        layouter.constrain_instance(serial_number.cell(), config.primary, 0)?;

        // Hash the treasury bulla
        let mut poseidon_message: Vec<AssignedCell<pallas::Base, pallas::Base>> =
            Vec::with_capacity(4);
        poseidon_message.push(contract_address);
        poseidon_message.push(serial_number);
        poseidon_message.push(treasury_balance);
        poseidon_message.push(bulla_blind);

        let hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<4>, 3, 2>::init(
            config.poseidon_chip(),
            layouter.namespace(|| "PoseidonHash init"),
        )?;

        let output = hasher.hash(
            layouter.namespace(|| "PoseidonHash hash"),
            poseidon_message.try_into().unwrap(),
        )?;

        let dao_bulla: AssignedCell<pallas::Base, pallas::Base> = output.into();

        // Constrain the merkle root
        let path: Option<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.merkle_path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            config.merkle_chip_1(),
            config.merkle_chip_2(),
            OrchardHashDomains::MerkleCrh,
            self.leaf_pos,
            path,
        );

        let root = merkle_inputs
            .calculate_root(layouter.namespace(|| "Calculate merkle root"), dao_bulla)?;

        println!("Merkle root in circuit: {:?}", root.value());
        layouter.constrain_instance(root.cell(), config.primary, 1)?;

        // Hash output 0
        let mut poseidon_message: Vec<AssignedCell<pallas::Base, pallas::Base>> =
            Vec::with_capacity(4);
        poseidon_message.push(output0_value);
        poseidon_message.push(output0_pub_x);
        poseidon_message.push(output0_pub_y);
        poseidon_message.push(output0_blind);

        let hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<4>, 3, 2>::init(
            config.poseidon_chip(),
            layouter.namespace(|| "PoseidonHash init"),
        )?;

        let output = hasher.hash(
            layouter.namespace(|| "PoseidonHash hash"),
            poseidon_message.try_into().unwrap(),
        )?;

        let output0: AssignedCell<pallas::Base, pallas::Base> = output.into();
        println!("Output0 in circuit: {:?}", output0.value());

        // Constrain output 0
        layouter.constrain_instance(output0.cell(), config.primary, 2)?;

        // Commit to votes with votes_blind
        let (commitment, _) = {
            let value_commit_v = ValueCommitV;
            let value_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), value_commit_v);
            value_commit_v.mul(layouter.namespace(|| "[value] ValueCommitV"), (votes, one))?
        };

        let (blind, _) = {
            let rcv = self.vote_blinds;
            let value_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let value_commit_r = FixedPoint::from_inner(ecc_chip.clone(), value_commit_r);
            value_commit_r.mul(layouter.namespace(|| "[value_blind] ValueCommitR"), rcv)?
        };

        // Constrain votes_commit_x and votes_commit_y
        let votes_commit = commitment.add(layouter.namespace(|| "valuecommit"), &blind)?;

        println!("VoteComX in circuit: {:?}", votes_commit.inner().x().value());
        println!("VoteComY in circuit: {:?}", votes_commit.inner().y().value());
        layouter.constrain_instance(votes_commit.inner().x().cell(), config.primary, 3)?;
        layouter.constrain_instance(votes_commit.inner().y().cell(), config.primary, 4)?;

        // TODO: Enforce votes > 0

        // TODO: Output 1 (change) = treasury_balance - output0_value

        // Commit to output 1 value
        // Constrain output1_commit_x and output1_commit_y

        debug!("Exiting synthesize()");
        Ok(())
    }
}

fn main() -> Result<()> {
    let loglevel = match option_env!("RUST_LOG") {
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        Some(_) | None => LevelFilter::Info,
    };
    TermLogger::init(loglevel, Config::default(), Mixed, Auto)?;

    /*
    let bincode = include_bytes!("../proof/dao.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;
    */

    // Contract address
    let a = pallas::Base::random(&mut OsRng);
    // Serial number
    let s = pallas::Base::random(&mut OsRng);
    // Money in treasury
    let t = pallas::Base::from(666);
    // Bulla blind
    let b_b = pallas::Base::random(&mut OsRng);

    let message = [a, s, t, b_b];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let bulla = hasher.hash(message);

    // Merkle tree of DAOs
    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    let dao0 = pallas::Base::random(&mut OsRng);
    let dao2 = pallas::Base::random(&mut OsRng);
    tree.append(&MerkleNode(dao0));
    tree.witness();
    tree.append(&MerkleNode(bulla));
    tree.witness();
    tree.append(&MerkleNode(dao2));
    tree.witness();

    let (leaf_pos, merkle_path) = tree.authentication_path(&MerkleNode(bulla)).unwrap();
    let leaf_pos: u64 = leaf_pos.into();
    let leaf_pos = leaf_pos as u32;

    // Output 0:
    let output0_val = pallas::Base::from(42);
    let output0_dest = pallas::Point::random(&mut OsRng);
    let output0_coords = output0_dest.to_affine().coordinates().unwrap();
    let output0_blind = pallas::Base::random(&mut OsRng);

    let message = [output0_val, *output0_coords.x(), *output0_coords.y(), output0_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let output0 = hasher.hash(message);

    let authority = Keypair::random(&mut OsRng);
    let _signature = authority.secret.sign(&output0.to_repr());

    let vote_1 = pallas::Base::from(44);
    let vote_2 = pallas::Base::from(13);
    // This is a NO vote
    let vote_3 = -pallas::Base::from(49);

    let vote_1_blind = pallas::Scalar::random(&mut OsRng);
    let vote_1_commit = pedersen_commitment_scalar(mod_r_p(vote_1), vote_1_blind);

    let vote_2_blind = pallas::Scalar::random(&mut OsRng);
    let vote_2_commit = pedersen_commitment_scalar(mod_r_p(vote_2), vote_2_blind);

    let vote_3_blind = pallas::Scalar::random(&mut OsRng);
    let vote_3_commit = pedersen_commitment_scalar(mod_r_p(vote_3), vote_3_blind);

    let vote_commit = vote_1_commit + vote_2_commit; //+ vote_3_commit;
    let vote_commit_coords = vote_commit.to_affine().coordinates().unwrap();

    let votes = vote_1 + vote_2; //+vote_3;
    let vote_blinds = vote_1_blind + vote_2_blind; //+ vote_3_blind;

    let output_1_blind = pallas::Scalar::random(&mut OsRng);

    /*
    let number = pallas::Base::from(u64::MAX).to_bytes();
    let bits = number.view_bits::<Lsb0>();
    println!("Positive: {:?}", bits);

    //let number = (-pallas::Base::from(u64::MAX)).to_bytes();
    let number = pallas::Base::from(0).to_bytes();
    let bits = number.view_bits::<Lsb0>();
    println!("Negative: {:?}", bits);
    */

    let circuit = ZkCircuit {
        a: Some(a),
        s: Some(s),
        t: Some(t),
        b_b: Some(b_b),
        leaf_pos: Some(leaf_pos),
        merkle_path: Some(merkle_path.try_into().unwrap()),
        u: Some(output0_val),
        p_x: Some(*output0_coords.x()),
        p_y: Some(*output0_coords.y()),
        b_m: Some(output0_blind),
        votes: Some(votes),
        vote_blinds: Some(vote_blinds),
        output_1_blind: Some(output_1_blind),
    };

    let public_inputs =
        vec![s, tree.root().inner(), output0, *vote_commit_coords.x(), *vote_commit_coords.y()];
    println!("{:#?}", public_inputs);

    let prover = MockProver::run(11, &circuit, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    Ok(())
}
