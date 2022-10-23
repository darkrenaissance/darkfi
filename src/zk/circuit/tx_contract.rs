use darkfi_sdk::crypto::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        util::gen_const_array,
        NullifierK, OrchardFixedBases, OrchardFixedBasesFull, MERKLE_DEPTH_ORCHARD,
    },
    MerkleNode,
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
pub struct TxConfig {
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

impl TxConfig {
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

const TX_COIN1_PK_OFFSET: usize = 0;
const TX_COIN1_SK_ROOT_OFFSET: usize = 1;
const TX_COIN2_PK_OFFSET: usize = 2;
const TX_COIN2_SK_ROOT_OFFSET: usize = 3;
const TX_COIN1_CM_X_OFFSET: usize = 4;
const TX_COIN1_CM_y_OFFSET: usize = 5;
const TX_COIN2_CM_X_OFFSET: usize = 6;
const TX_COIN2_CM_y_OFFSET: usize = 7;
const TX_COIN3_CM_X_OFFSET: usize = 8;
const TX_COIN3_CM_y_OFFSET: usize = 9;
const TX_COIN4_CM_X_OFFSET: usize = 10;
const TX_COIN4_CM_y_OFFSET: usize = 11;
const TX_COIN1_SN_OFFSET: usize = 12;
const TX_COIN2_SN_OFFSET: usize = 13;

#[derive(Default, Debug)]
pub struct TxContract {
    // witness
    pub root_cm: Value<pallas::Base>, // root to coins commitment
    //
    pub coin1_root_sk: Value<pallas::Base>, // coins merkle tree secret key of coin1
    pub coin1_sk_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to coin1 sk
    pub coin1_sk_pos: Value<u32>,

    pub coin2_root_sk: Value<pallas::Base>, // coins merkle tree secret key of coin2
    pub coin2_sk_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to coin2 sk
    pub coin2_cm_pos: Value<u32>,

    pub coin3_pk: Value<pallas::Point>,
    pub coin4_pk: VAlue<pallas::Point>,

    pub coin1_nonce: Value<pallas::Base>,
    pub coin1_blind: Value<pallas::Scalar>,
    pub coin1_value: Value<pallas::Base>,
    pub coin1_cm_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to coin1 cm
    pub coin1_cm_pos: Value<u32>,

    pub coin2_nonce: Value<pallas::Base>,
    pub coin2_blind: Value<pallas::Scalar>,
    pub coin2_value: Value<pallas::Base>,
    pub coin2_cm_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to coin2 cm
    pub coin2_cm_pos: Value<u32>,

    pub coin3_nonce: Value<pallas::Base>,
    pub coin3_blind: Value<pallas::Scalar>,
    pub coin3_value: Value<pallas::Base>,

    pub coin4_nonce: Value<pallas::Base>,
    pub coin4_blind: Value<pallas::Scalar>,
    pub coin4_value: Value<pallas::Base>,
}

impl UtilitiesInstructions<pallas::Base> for TxContract {
    type Var = AssignedCell<Fp, Fp>;
}

impl Circuit<pallas::Base> for TxContract {
    type Config = TxConfig;
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

        TxConfig {
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
        NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>
            ::load_k_table(
                &mut layouter,
                config.lessthan_config.k_values_table,
            )?;
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;
        let ecc_chip = config.ecc_chip();
        let ar_chip = config.arith_chip();

        // ================
        // load witnesses
        // ================
        let one = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Value::known(pallas::Base::one()),
        )?;
        let neg_one = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Value::known(-pallas::Base::one()),
        )?;
        let root_cm = self.load_private(layouter.namespace(|| ""),
                                        config.advices[0],
                                        self.root_cm
        )?;
        let coin1_root_sk = self.load_private(layouter.namespace(|| "root sk"),
                                              config.advices[0],
                                              self.coin1_root_sk
        )?;

        let coin1_value = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin1_value
        )?;

        let coin1_nonce = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin1_nonce
        )?;


        let coin1_cm_path = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin1_cm_path
        )?;

        let coin2_root_sk = self.load_private(layouter.namespace(|| "root sk"),
                                              config.advices[0],
                                              self.coin2_root_sk
        )?;

        let coin2_value = self.load_private(layouter.namespace(|| ""),
                                              config.advices[0],
                                              self.coin2_value
        )?;

        let coin2_nonce = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin2_nonce
        )?;

        let coin2_cm_path = self.load_private(layouter.namespace(|| ""),
                                              config.advices[0],
                                              self.coin2_cm_path
        )?;

        let coin3_value = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin3_value
        )?;

        let coin3_nonce = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin3_nonce
        )?;


        let coin4_value = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin4_value
        )?;

        let coin4_nonce = self.load_private(layouter.namespace(|| ""),
                                            config.advices[0],
                                            self.coin4_nonce
        )?;

        // ===========
        // proof
        // ===========

        // ========
        // coin1 pk
        // ========
        let coin1_pk: AssignedCell<Fp, Fp> = {
            let poseidon_message = [one.clone(), coin1_root_sk.clone()];
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

        layouter.constrain_instance(
            coin1_pk.cell(),
            config.primary,
            TX_COIN1_PK_OFFSET,
        )?;

        // ========
        // coin2 pk
        // ========
        let coin2_pk: AssignedCell<Fp, Fp> = {
            let poseidon_message = [one.clone(), coin2_root_sk.clone()];
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

        layouter.constrain_instance(
            coin2_pk.cell(),
            config.primary,
            TX_COIN2_PK_OFFSET,
        )?;
        // ========
        // coin1 cm
        // ========
        let com1 = {
            let nullifier2_msg: AssignedCell<Fp, Fp> = {
                let poseidon_message = [
                    coin1_pk.clone(),
                    coin1_value.clone(),
                    coin1_nonce.clone(),
                    one.clone()
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
            let coin1_blind = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin blind scalar"),
                self.coin1_blind,
            )?;
            let coin_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), coin1_blind)?
        };
        let coin1_commit = com2.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin1_commit_x: AssignedCell<Fp, Fp> = coin1_commit.inner().x();
        let coin1_commit_y: AssignedCell<Fp, Fp> = coin1_commit.inner().y();
        layouter.constrain_instance(
            coin1_commit_x.cell(),
            config.primary,
            TX_COIN1_CM_X_OFFSET,
        )?;
        layouter.constrain_instance(
            coin1_commit_y.cell(),
            config.primary,
            TX_COIN1_CM_Y_OFFSET,
        )?;
        // ========
        // coin2 cm
        // ========
        let com2 = {
            let nullifier2_msg: AssignedCell<Fp, Fp> = {
                let poseidon_message = [
                    coin2_pk.clone(),
                    coin2_value.clone(),
                    coin2_nonce.clone(),
                    one.clone()
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
                layouter.namespace(|| "coin blind scalar"),
                self.coin2_blind,
            )?;
            let coin_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), coin2_blind)?
        };
        let coin2_commit = com2.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin2_commit_x: AssignedCell<Fp, Fp> = coin2_commit.inner().x();
        let coin2_commit_y: AssignedCell<Fp, Fp> = coin2_commit.inner().y();
        layouter.constrain_instance(
            coin2_commit_x.cell(),
            config.primary,
            TX_COIN2_CM_X_OFFSET,
        )?;
        layouter.constrain_instance(
            coin2_commit_y.cell(),
            config.primary,
            TX_COIN2_CM_Y_OFFSET,
        )?;
        // ========
        // coin3 cm
        // ========
        let com3 = {
            let nullifier2_msg: AssignedCell<Fp, Fp> = {
                let poseidon_message = [
                    coin3_pk.clone(),
                    coin3_value.clone(),
                    coin3_nonce.clone(),
                    one.clone()
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
            let coin3_blind = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin blind scalar"),
                self.coin3_blind,
            )?;
            let coin_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), coin3_blind)?
        };
        let coin3_commit = com2.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin3_commit_x: AssignedCell<Fp, Fp> = coin3_commit.inner().x();
        let coin3_commit_y: AssignedCell<Fp, Fp> = coin3_commit.inner().y();
        layouter.constrain_instance(
            coin3_commit_x.cell(),
            config.primary,
            TX_COIN3_CM_X_OFFSET,
        )?;
        layouter.constrain_instance(
            coin3_commit_y.cell(),
            config.primary,
            TX_COIN3_CM_Y_OFFSET,
        )?;
        // ========
        // coin4 cm
        // ========
        let com4 = {
            let nullifier2_msg: AssignedCell<Fp, Fp> = {
                let poseidon_message = [
                    coin4_pk.clone(),
                    coin4_value.clone(),
                    coin4_nonce.clone(),
                    one.clone()
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
            let coin4_blind = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin blind scalar"),
                self.coin4_blind,
            )?;
            let coin_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), coin4_blind)?
        };
        let coin4_commit = com2.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin4_commit_x: AssignedCell<Fp, Fp> = coin4_commit.inner().x();
        let coin4_commit_y: AssignedCell<Fp, Fp> = coin4_commit.inner().y();
        layouter.constrain_instance(
            coin4_commit_x.cell(),
            config.primary,
            TX_COIN4_CM_X_OFFSET,
        )?;
        layouter.constrain_instance(
            coin4_commit_y.cell(),
            config.primary,
            TX_COIN4_CM_Y_OFFSET,
        )?;

        let v1pv2 = ar_chip.add(layouter.namespace(||""), &coin1_value, &coin2_value)?;
        let v3pv4 = ar_chip.add(layouter.namespace(||""), &coin3_value, &coin4_value)?;
        v1pv2.constrain_equal(layouter.namespace(||""), &v1pv2)?;
        // ==========
        // COIN1 PATH
        // ==========
        // path to coin1_cm in the tree with root coin1_root
        let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.coin1_cm_path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));
        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.coin1_cm_pos,
            path,
        );

        let coin1_cm_hash: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin1_commit_x.clone(), coin1_commit_y.clone()];
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
        let coin1_cm_root = merkle_inputs
            .calculate_root(layouter.namespace(|| "calculate root"), coin1_cm_hash)?;
        root_cm.constrain_equal(layouter.namespace(||""), &coin1_cm_root)?;

        // ==========
        // COIN2 PATH
        // ==========
        // path to coin2_cm in the tree with root coin2_root
        let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.coin2_cm_path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));
        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.coin2_cm_pos,
            path,
        );

        let coin2_cm_hash: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin2_commit_x.clone(), coin2_commit_y.clone()];
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
        let coin2_cm_root = merkle_inputs
            .calculate_root(layouter.namespace(|| "calculate root"), coin2_cm_hash)?;
        root_cm.constrain_equal(layouter.namespace(||""), &coin2_cm_root)?;
        // =============
        // COIN1 sk root
        // =============
        let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.coin1_sk_path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));
        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.coin1_sk_pos,
            path,
        );
        let coin2_cm_root = merkle_inputs
            .calculate_root(layouter.namespace(|| "calculate root"), coin1_sk)?;
        layouter.constrain_instance(
            coin1_root_sk.cell(),
            config.primary,
            TX_COIN1_CM_ROOT_OFFSET,
        )?;
        // =============
        // COIN2 sk root
        // =============
        layouter.constrain_instance(
            coin2_root_sk.cell(),
            config.primary,
            TX_COIN2_CM_ROOT_OFFSET,
        )?;
        // ========
        // coin1 sn
        // ========
        let coin1_sn_commit: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin1_nonce.clone(), coin1_root_sk.clone()];
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
        layouter.constrain_instance(
            coin1_sn_commit.cell(),
            config.primary,
            TX_COIN1_SN_OFFSET,
        )?;
        // ========
        // coin2 sn
        // ========
        let coin2_sn_commit: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin2_nonce.clone(), coin2_root_sk.clone()];
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
        layouter.constrain_instance(
            coin2_sn_commit.cell(),
            config.primary,
            TX_COIN2_SN_OFFSET,
        )?;
    }
}
