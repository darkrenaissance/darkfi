// https://docs.vocdoni.io/architecture/protocol/anonymous-voting/zk-census-proof.html#protocol-design
use anyhow::Result;
use halo2::{
    circuit::{Layouter, SimpleFloorPlanner},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};
use halo2_gadgets::{
    ecc::chip::{EccChip, EccConfig},
    poseidon::{Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig},
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::chip::{MerkleChip, MerkleConfig},
    },
    utilities::{lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions},
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{ff::PrimeFieldBits, Curve},
    pallas,
};
use rand::rngs::OsRng;

use drk_halo2::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains, MERKLE_CRH_PERSONALIZATION},
        OrchardFixedBases,
    },
    crypto::pedersen_commitment,
    spec::i2lebsp,
};

#[derive(Clone, Debug)]
struct VoteConfig {
    primary: Column<InstanceColumn>,
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

impl VoteConfig {
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

// The public input array offsets
const VOTE_MERKLE_ROOT_OFFSET: usize = 0;
const VOTE_NULLIFIER_OFFSET: usize = 1;
const VOTE_PROCESS_ID_OFFSET: usize = 2;
const VOTE_COMMITX_OFFSET: usize = 3;
const VOTE_COMMITY_OFFSET: usize = 4;

#[derive(Default, Debug)]
struct VoteCircuit {
    index: Option<pallas::Base>,
    secret_key: Option<pallas::Base>,
    merkle_proof: Option<pallas::Base>,
}

impl UtilitiesInstructions<pallas::Base> for VoteCircuit {
    type Var = CellValue<pallas::Base>;
}

impl Circuit<pallas::Base> for VoteCircuit {
    type Config = VoteConfig;
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

        VoteConfig {
            primary,
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

fn main() -> Result<()> {
    // The number of rows in our circuit cannot exceed 2^k
    // let k: u32 = 11;

    // Voter is the owner of the secret key corresponding to a certain zkCensusKey.
    let secret_key = pallas::Base::random(&mut OsRng);
    let zk_census_key =
        primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<1>).hash([secret_key]);

    // Voter's zkCensusKey is included in the census Merkle Tree
    let leaf = zk_census_key.clone();
    let pos = rand::random::<u32>();
    let path: Vec<_> = (0..32).map(|_| pallas::Base::random(&mut OsRng)).collect();
    let merkle_root = root(path.clone().try_into().unwrap(), pos, leaf);

    // The nullifier provided by Voter uniquely corresponds to their secret
    // key and the process ID for a specific voting process.
    let process_id = pallas::Base::from(42);
    let nullifier = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>)
        .hash([secret_key, process_id]);

    // The vote itself
    let vote_blind = pallas::Scalar::random(&mut OsRng);
    let vote = pedersen_commitment(1, vote_blind);
    let vote_coords = vote.to_affine().coordinates().unwrap();

    let _public_inputs = [
        merkle_root,
        nullifier,
        process_id,
        *vote_coords.x(),
        *vote_coords.y(),
    ];

    Ok(())
}
