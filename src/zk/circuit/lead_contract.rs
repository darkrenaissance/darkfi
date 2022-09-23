use crate::crypto::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        util::gen_const_array,
        NullifierK, OrchardFixedBases, OrchardFixedBasesFull, MERKLE_DEPTH_ORCHARD,
    },
    merkle_node::MerkleNode,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointBaseField, ScalarFixed,
    },
    poseidon::{
        primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
        Pow5Config as PoseidonConfig,
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
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};
use pasta_curves::{pallas, Fp};

use crate::zk::gadget::{
    arithmetic::{ArithChip, ArithConfig, ArithInstruction},
    //even_bits::{EvenBitsChip, EvenBitsConfig, EvenBitsLookup},
    less_than::{LessThanChip, LessThanConfig},
    native_range_check::NativeRangeCheckChip,
};

const WINDOW_SIZE: usize = 3;
const NUM_OF_BITS: usize = 254;
const NUM_OF_WINDOWS: usize = 85;

const PRF_NULLIFIER_PREFIX: u64 = 0;

#[derive(Clone, Debug)]
pub struct LeadConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    _sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,

    lessthan_config: LessThanConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>,

    arith_config: ArithConfig,
}

impl LeadConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
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

    fn lessthan_chip(&self) -> LessThanChip<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS> {
        LessThanChip::construct(self.lessthan_config.clone())
    }

    fn arith_chip(&self) -> ArithChip {
        ArithChip::construct(self.arith_config.clone())
    }
}

const LEAD_COIN_COMMIT_X_OFFSET: usize = 0;
const LEAD_COIN_COMMIT_Y_OFFSET: usize = 1;
const LEAD_COIN_COMMIT2_X_OFFSET: usize = 2;
const LEAD_COIN_COMMIT2_Y_OFFSET: usize = 3;
const LEAD_COIN_NONCE2_OFFSET: usize = 4;
const LEAD_COIN_COMMIT_PATH_OFFSET: usize = 5;
const LEAD_COIN_PK_OFFSET: usize = 6;
const LEAD_COIN_SERIAL_NUMBER_OFFSET: usize = 7;
const LEAD_Y_COMMIT_BASE_OFFSET: usize = 8;
const LEAD_RHO_COMMIT_BASE_OFFSET: usize = 9;

pub fn concat_u8(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    [lhs, rhs].concat()
}

#[derive(Default, Debug)]
pub struct LeadContract {
    // witness
    pub path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub root_sk: Value<pallas::Base>, // coins merkle tree secret key of coin1
    pub path_sk: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the secret key root_sk
    pub coin_timestamp: Value<pallas::Base>,
    pub coin_nonce: Value<pallas::Base>,
    pub coin1_blind: Value<pallas::Scalar>,
    pub value: Value<pallas::Base>,
    pub coin2_blind: Value<pallas::Scalar>,
    // public advices
    pub cm_pos: Value<u32>,
    //
    //pub sn_c1 : Option<pallas::Base>,
    pub slot: Value<pallas::Base>,
    pub mau_rho: Value<pallas::Scalar>,
    pub mau_y: Value<pallas::Scalar>,
    pub root_cm: Value<pallas::Scalar>,
    pub sigma_scalar: Value<pallas::Base>,
    //pub eta : Option<u32>,
    //pub rho : Option<u32>,
    //pub h : Option<u32>, // hash of this data
    //pub ptr: Option<u32>, //hash of the previous block
}

impl UtilitiesInstructions<pallas::Base> for LeadContract {
    type Var = AssignedCell<Fp, Fp>;
}

impl Circuit<pallas::Base> for LeadContract {
    type Config = LeadConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
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

        let table_idx = meta.lookup_table_column();
        let lookup = (table_idx, meta.lookup_table_column(), meta.lookup_table_column());

        let primary = meta.instance_column();
        meta.enable_equality(primary);

        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

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

        meta.enable_constant(lagrange_coeffs[0]);
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices[0..10].try_into().expect("wrong slice size"),
            lagrange_coeffs,
            range_check,
        );

        let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
            meta,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        let (sinsemilla_config_1, merkle_config_1) = {
            let sinsemilla_config_1 = SinsemillaChip::configure(
                meta,
                advices[..5].try_into().unwrap(),
                advices[6],
                lagrange_coeffs[0],
                lookup,
                range_check,
            );
            let merkle_config_1 = MerkleChip::configure(meta, sinsemilla_config_1.clone());
            (sinsemilla_config_1, merkle_config_1)
        };

        let (sinsemilla_config_2, merkle_config_2) = {
            let sinsemilla_config_2 = SinsemillaChip::configure(
                meta,
                advices[5..10].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2.clone());

            (sinsemilla_config_2, merkle_config_2)
        };

        let k_values_table = meta.lookup_table_column();

        let lessthan_config = {
            let a = meta.advice_column();
            let b = meta.advice_column();
            let a_offset = meta.advice_column();

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            LessThanChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::configure(
                meta,
                a,
                b,
                a_offset,
                k_values_table,
            )
        };

        let arith_config = ArithChip::configure(meta, advices[7], advices[8], advices[6]);

        LeadConfig {
            primary,
            advices,
            ecc_config,
            poseidon_config,
            merkle_config_1,
            merkle_config_2,
            sinsemilla_config_1,
            _sinsemilla_config_2: sinsemilla_config_2,
            lessthan_config,
            arith_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        let less_than_chip = config.lessthan_chip();
        NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::load_k_table(
            &mut layouter,
            config.lessthan_config.k_values_table,
        )?;
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;
        let ecc_chip = config.ecc_chip();
        let ar_chip = config.arith_chip();
        let _ps_chip = config.poseidon_chip();

        // ===============
        // load witnesses
        // ===============

        // constant identity value 1
        let one = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Value::known(pallas::Base::one()),
        )?;

        // prefix to the pseudo-random-function that prefix input
        // to the nullifier poseidon hash
        let prf_nullifier_prefix_base = self.load_private(
            layouter.namespace(|| "PRF NULLIFIER PREFIX BASE"),
            config.advices[0],
            Value::known(pallas::Base::from(PRF_NULLIFIER_PREFIX)),
        )?;
        // staking coin timestamp
        let coin_timestamp = self.load_private(
            layouter.namespace(|| "load coin time stamp"),
            config.advices[0],
            self.coin_timestamp,
        )?;

        // staking coin nonce
        let coin_nonce: AssignedCell<Fp, Fp> = self.load_private(
            layouter.namespace(|| "load coin nonce"),
            config.advices[0],
            self.coin_nonce,
        )?;

        // staking coin value
        let coin_value = self.load_private(
            layouter.namespace(|| "load coin value"),
            config.advices[0],
            self.value,
        )?;

        // staking coin secret key
        let _root_sk =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.root_sk)?;

        // sigma scalar is 2^254/(total network stake + epsilon)
        let sigma_scalar = self.load_private(
            layouter.namespace(|| "load scalar "),
            config.advices[0],
            self.sigma_scalar,
        )?;

        // leadership coefficient used for fine-tunning leader election frequency
        let _c = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            Value::known(pallas::Base::one()), // note! this parameter to be tuned.
        )?;

        // coin public key pk=PRF_{root_sk}(tau)
        // coin public key is pseudo random hash of concatenation of the following:
        // coin timestamp, and root of coin's secret key.
        let coin_pk_commit: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin_timestamp.clone(), _root_sk.clone()];
            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        // coin c1 serial number sn=PRF_{root_sk}(nonce)
        // coin's serial number is derived from coin nonce (sampled at random)
        // and root of the coin's secret key sampled an random.
        let sn_commit: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin_nonce.clone(), _root_sk.clone()];
            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        // commitment to the staking coin
        // coin commiment H=COMMIT(PRF(prefix||pk||V||nonce), r)
        let com = {
            // coin c1 nullifier is a commitment of the following
            // nullifier input
            let nullifier_msg: AssignedCell<Fp, Fp> = {
                let poseidon_message = [
                    prf_nullifier_prefix_base.clone(),
                    coin_pk_commit.clone(),
                    coin_value.clone(),
                    coin_nonce.clone(),
                ];
                let poseidon_hasher = PoseidonHash::<
                    _,
                    _,
                    poseidon::P128Pow5T3,
                    poseidon::ConstantLength<4>,
                    3,
                    2,
                >::init(
                    config.poseidon_chip(),
                    layouter.namespace(|| "Poseidon init"),
                )?;

                let poseidon_output = poseidon_hasher
                    .hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
                let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
                poseidon_output
            };
            let coin_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            coin_commit_v.mul(layouter.namespace(|| "coin commit v"), nullifier_msg)?
        };

        // r*G_2
        let (blind, _) = {
            let rcv = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin1 blind scalar"),
                self.coin1_blind,
            )?;
            let coin_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), rcv)?
        };

        let coin_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin_commit_x: AssignedCell<Fp, Fp> = coin_commit.inner().x();
        let coin_commit_y: AssignedCell<Fp, Fp> = coin_commit.inner().y();

        // nonce2  =  PRF_{root_sk}(coin_nonce)
        // poured coin derived nonce as a poseidon of the previous nonce, and
        // root of secret key.
        let coin2_nonce: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin_nonce.clone(), _root_sk.clone()];
            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        // coin2 commiment H=COMMIT(PRF(pk||V||nonce2), r2)
        // poured coin's commitment is a nullifier
        let com2 = {
            // coin2's commitment input body as a poseidon of input concatenation of
            // public key, stake, and poured coin's nonce.
            let nullifier2_msg: AssignedCell<Fp, Fp> = {
                let poseidon_message = [
                    prf_nullifier_prefix_base.clone(),
                    coin_pk_commit.clone(),
                    coin_value.clone(),
                    coin2_nonce.clone(),
                ];
                let poseidon_hasher = PoseidonHash::<
                    _,
                    _,
                    poseidon::P128Pow5T3,
                    poseidon::ConstantLength<4>,
                    3,
                    2,
                >::init(
                    config.poseidon_chip(),
                    layouter.namespace(|| "Poseidon init"),
                )?;

                let poseidon_output = poseidon_hasher
                    .hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
                let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
                poseidon_output
            };
            let coin_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            coin_commit_v.mul(layouter.namespace(|| "coin commit v"), nullifier2_msg)?
        };
        // r*G_2
        let (blind, _) = {
            let coin2_blind = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin2 blind scalar"),
                self.coin2_blind,
            )?;
            let coin_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), coin2_blind)?
        };
        let coin2_commit = com2.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin2_commit_x: AssignedCell<Fp, Fp> = coin2_commit.inner().x();
        let coin2_commit_y: AssignedCell<Fp, Fp> = coin2_commit.inner().y();

        // path is valid path to staked coin's commitment
        let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.cm_pos,
            path,
        );

        let coin_commit_prod: AssignedCell<Fp, Fp> = {
            let coin_commit_coordinates = coin_commit.inner();

            let res: AssignedCell<Fp, Fp> = ar_chip.mul(
                layouter.namespace(|| ""),
                &coin_commit_coordinates.x(),
                &coin_commit_coordinates.y(),
            )?;
            res
        };
        let computed_final_root = merkle_inputs
            .calculate_root(layouter.namespace(|| "calculate root"), coin_commit_prod)?;

        // lhs of the leader election lottery
        // *  y as COMIT(root_sk||nonce, mau_y)
        // beging the commitment to the coin's secret key, coin's nonce, and
        // random value deriven from the epoch sampled random eta.
        let lottery_commit_msg: AssignedCell<Fp, Fp> = {
            let poseidon_message = [_root_sk.clone(), coin_nonce.clone()];
            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        let com = {
            let y_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            y_commit_v.mul(layouter.namespace(|| "coin commit v"), lottery_commit_msg)?
        };

        // r*G_2
        let (blind, _) = {
            let mau_y = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "mau_y scalar"),
                self.mau_y,
            )?;
            let y_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            y_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), mau_y)?
        };
        let y_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let y_commit_base = y_commit.inner().x();

        // constraint rho as COMIT(PRF(root_sk||nonce), rho_mu)
        // r*G_2
        let (blind, _) = {
            let mau_rho = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "mau_rho scalar"),
                self.mau_rho,
            )?;
            let rho_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            rho_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), mau_rho)?
        };
        let rho_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let rho_commit_base = rho_commit.inner().x();
        // stakeholder absolute stake + 1 (epsilon)
        let stake_plus = ar_chip.add(layouter.namespace(|| ""), &one, &coin_value.clone())?;
        let target =
            ar_chip.mul(layouter.namespace(|| "calculate target"), &sigma_scalar, &stake_plus)?;

        let y: Value<pallas::Base> = y_commit_base.value().cloned();
        let target: Value<pallas::Base> = target.value().cloned();
        less_than_chip.witness_less_than(
            layouter.namespace(|| "y < target"),
            target, //reversed for testing
            y,
            0,
            true,
        )?;

        layouter.constrain_instance(
            coin_commit_x.cell(),
            config.primary,
            LEAD_COIN_COMMIT_X_OFFSET,
        )?;

        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin_commit_y.cell(),
            config.primary,
            LEAD_COIN_COMMIT_Y_OFFSET,
        )?;

        layouter.constrain_instance(
            coin2_commit_x.cell(),
            config.primary,
            LEAD_COIN_COMMIT2_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin2_commit_y.cell(),
            config.primary,
            LEAD_COIN_COMMIT2_Y_OFFSET,
        )?;

        layouter.constrain_instance(
            coin2_nonce.clone().cell(),
            config.primary,
            LEAD_COIN_NONCE2_OFFSET,
        )?;

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            LEAD_COIN_COMMIT_PATH_OFFSET,
        )?;

        layouter.constrain_instance(coin_pk_commit.cell(), config.primary, LEAD_COIN_PK_OFFSET)?;

        // constrain coin's pub key x value
        layouter.constrain_instance(
            sn_commit.cell(),
            config.primary,
            LEAD_COIN_SERIAL_NUMBER_OFFSET,
        )?;

        layouter.constrain_instance(
            y_commit_base.cell(),
            config.primary,
            LEAD_Y_COMMIT_BASE_OFFSET,
        )?;

        layouter.constrain_instance(
            rho_commit_base.cell(),
            config.primary,
            LEAD_RHO_COMMIT_BASE_OFFSET,
        )?;

        Ok(())
    }
}
