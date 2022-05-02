use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointShort,
    },
    poseidon::{Hash as PoseidonHash, Pow5Chip as PoseidonChip, Pow5Config as PoseidonConfig},
    primitives::poseidon::{ConstantLength, P128Pow5T3},
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
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};

use pasta_curves::{pallas, Fp};

use crate::crypto::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        util::gen_const_array,
        OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
    },
    merkle_node::MerkleNode,
};

use crate::zk::{
    arith_chip::{ArithmeticChip, ArithmeticChipConfig},
    even_bits::{EvenBitsChip, EvenBitsConfig, EvenBitsLookup},
    greater_than::{GreaterThanChip, GreaterThanConfig, GreaterThanInstruction},
};

//use pasta_curves::{arithmetic::CurveAffine, group::Curve};
//use halo2_proofs::arithmetic::CurveAffine;
use pasta_curves::group::{ff::PrimeField, GroupEncoding};

const WORD_BITS: u32 = 24;

#[derive(Clone, Debug)]
pub struct LeadConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 12],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    greaterthan_config: GreaterThanConfig,
    evenbits_config: EvenBitsConfig,
    arith_config: ArithmeticChipConfig,
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

    fn greaterthan_chip(&self) -> GreaterThanChip<pallas::Base, WORD_BITS> {
        GreaterThanChip::construct(self.greaterthan_config.clone())
    }

    fn evenbits_chip(&self) -> EvenBitsChip<pallas::Base, WORD_BITS> {
        EvenBitsChip::construct(self.evenbits_config.clone())
    }

    fn arith_chip(&self) -> ArithmeticChip {
        ArithmeticChip::construct(self.arith_config.clone())
    }
}

const LEAD_COIN_NONCE2_X_OFFSET: usize = 0;
const LEAD_COIN_NONCE2_Y_OFFSET: usize = 1;
const LEAD_COIN_PK_X_OFFSET: usize = 2;
const LEAD_COIN_PK_Y_OFFSET: usize = 3;
const LEAD_COIN_SERIAL_NUMBER_X_OFFSET: usize = 4;
const LEAD_COIN_SERIAL_NUMBER_Y_OFFSET: usize = 5;
const LEAD_COIN_COMMIT_X_OFFSET: usize = 6;
const LEAD_COIN_COMMIT_Y_OFFSET: usize = 7;
const LEAD_COIN_COMMIT2_X_OFFSET: usize = 8;
const LEAD_COIN_COMMIT2_Y_OFFSET: usize = 9;
const LEAD_COIN_COMMIT_PATH_OFFSET: usize = 10;
const LEAD_THRESHOLD_OFFSET: usize = 11;

#[derive(Debug, Default)]
pub struct LeadContract {
    // witness
    pub path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub root_sk: Option<pallas::Scalar>, // coins merkle tree secret key of coin1
    pub path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the secret key root_sk
    pub coin_timestamp: Option<pallas::Base>,
    pub coin_nonce: Option<pallas::Base>,
    pub coin_opening_1: Option<pallas::Scalar>,
    pub value: Option<pallas::Base>,
    pub coin_opening_2: Option<pallas::Scalar>,
    // public advices
    //
    //TODO implement two version of load_private one or point, other for base
    // or templated load_private. then you would be able to read (x,y) from cm_c
    pub cm_c1_x: Option<pallas::Base>,
    pub cm_c1_y: Option<pallas::Base>,
    //
    pub cm_c2_x: Option<pallas::Base>,
    pub cm_c2_y: Option<pallas::Base>,
    //
    pub cm_pos: Option<u32>,
    //
    //pub sn_c1 : Option<pallas::Base>,
    pub slot: Option<pallas::Base>,
    pub mau_rho: Option<pallas::Scalar>,
    pub mau_y: Option<pallas::Scalar>,
    pub root_cm: Option<pallas::Scalar>,
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

        let poseidon_config = PoseidonChip::configure::<P128Pow5T3>(
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

        let greaterthan_config = GreaterThanChip::<pallas::Base, WORD_BITS>::configure(
            meta,
            advices[10..12].try_into().unwrap(),
            primary,
        );
        let evenbits_config = EvenBitsChip::<pallas::Base, WORD_BITS>::configure(meta);
        let arith_config = ArithmeticChip::configure(meta);

        LeadConfig {
            primary,
            advices,
            ecc_config,
            poseidon_config,
            merkle_config_1,
            merkle_config_2,
            sinsemilla_config_1,
            sinsemilla_config_2,
            greaterthan_config,
            evenbits_config,
            arith_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;
        let ecc_chip = config.ecc_chip();
        let ar_chip = config.arith_chip();
        let ps_chip = config.poseidon_chip();
        let eb_chip = config.evenbits_chip();
        let greater_than_chip = config.greaterthan_chip();

        eb_chip.alloc_table(&mut layouter.namespace(|| "alloc table"))?;

        // ===============
        // load witnesses
        // ===============

        // coin_timestamp tau

        let coin_timestamp = self.load_private(
            layouter.namespace(|| "load coin time stamp"),
            config.advices[0],
            self.coin_timestamp,
        )?;

        // root of coin

        /*
        let root_sk = self.load_private(
            layouter.namespace(|| "load root coin"),
            config.advices[0],
            self.root_sk,
        )?;
        */
        // coin nonce

        let coin_nonce = self.load_private(
            layouter.namespace(|| "load coin nonce"),
            config.advices[0],
            self.coin_nonce,
        )?;

        let coin_value = self.load_private(
            layouter.namespace(|| "load opening 1"),
            config.advices[0],
            self.value,
        )?;

        /*
        let coin_opening_1 = self.load_private(
            layouter.namespace(|| "load opening 1"),
            config.advices[0],
            self.coin_opening_1,
        )?;
        let coin_opening_2 = self.load_private(
            layouter.namespace(|| "load opening 2"),
            config.advices[0],
            self.coin_opening_2,
        )?;
         */

        //let cm_c1_point : pallas::Point = pallas::Point::from(1);
        //let cm_c1 : AssignedCell<pallas::Point, pallas::Point> = cm_c1_point;

        let cm_c1_x =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.cm_c1_x)?;
        let cm_c1_y =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.cm_c1_y)?;

        let cm_c2_x =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.cm_c2_x)?;
        let cm_c2_y =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.cm_c2_y)?;

        /*
        let cm_pos = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.cm_pos
        )?;

        let sn_c1 = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.sn_c1,
        )?;
        */

        /*
        let eta = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.eta,
        )?;
         */

        let slot = self.load_private(layouter.namespace(|| ""), config.advices[0], self.slot)?;

        /*
        let rho = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.rho,
        )?;

        let h = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.h,
        )?;

        let ptr = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.ptr,
        )?;
         */

        /*
        let mau_rho = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.mau_rho,
        )?;

        let mau_y = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.mau_y,
        )?;
        */

        /*
        let root = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.root,
        )?;
         */

        let one = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Some(pallas::Base::one()),
        )?;
        //TODO read the second coin commitment as constant(public input)
        // in this case
        //

        // ===============
        // coin 2 nonce
        // ===============
        // m*G_1
        let (com, _) = {
            let nonce2_commit_v = ValueCommitV;
            let nonce2_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), nonce2_commit_v);
            nonce2_commit_v
                .mul(layouter.namespace(|| "coin_pk commit v"), (coin_nonce.clone(), one.clone()))?
        };
        // r*G_2
        let (blind, _) = {
            let nonce2_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let nonce2_commit_r = FixedPoint::from_inner(ecc_chip.clone(), nonce2_commit_r);
            nonce2_commit_r.mul(layouter.namespace(|| "nonce2 commit R"), self.root_sk)?
        };
        let coin2_nonce = com.add(layouter.namespace(|| "nonce2 commit"), &blind)?;

        layouter.constrain_instance(
            coin2_nonce.inner().x().cell(),
            config.primary,
            LEAD_COIN_NONCE2_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin2_nonce.inner().y().cell(),
            config.primary,
            LEAD_COIN_NONCE2_Y_OFFSET,
        )?;

        // ================
        // coin public key constraints derived from the coin timestamp
        // ================

        // m*G_1
        let (com, _) = {
            let coin_pk_commit_v = ValueCommitV;
            let coin_pk_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_pk_commit_v);
            coin_pk_commit_v
                .mul(layouter.namespace(|| "coin_pk commit v"), (coin_timestamp, one.clone()))?
        };
        // r*G_2
        let (blind, _) = {
            let coin_pk_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_pk_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_pk_commit_r);
            coin_pk_commit_r.mul(layouter.namespace(|| "coin_pk commit R"), self.root_sk)?
        };
        let coin_pk_commit = com.add(layouter.namespace(|| "coin timestamp commit"), &blind)?;
        // constrain coin's pub key x value

        layouter.constrain_instance(
            coin_pk_commit.inner().x().cell(),
            config.primary,
            LEAD_COIN_PK_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin_pk_commit.inner().y().cell(),
            config.primary,
            LEAD_COIN_PK_Y_OFFSET,
        )?;

        // =================
        // nonce constraints derived from previous coin's nonce
        // =================

        // =============
        // constrain coin c1 serial number
        // =============
        // m*G_1
        let (com, _) = {
            let sn_commit_v = ValueCommitV;
            let sn_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), sn_commit_v);
            sn_commit_v.mul(
                layouter.namespace(|| "coin serial number commit v"),
                (coin_nonce.clone(), one.clone()),
            )?
        };
        // r*G_2
        let (blind, _) = {
            let sn_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let sn_commit_r = FixedPoint::from_inner(ecc_chip.clone(), sn_commit_r);
            sn_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), self.root_sk)?
        };
        //
        let sn_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        // constrain coin's pub key x value

        layouter.constrain_instance(
            sn_commit.inner().x().cell(),
            config.primary,
            LEAD_COIN_SERIAL_NUMBER_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            sn_commit.inner().y().cell(),
            config.primary,
            LEAD_COIN_SERIAL_NUMBER_Y_OFFSET,
        )?;
        // ==========================
        // commitment of coins c1,c2
        // ==========================
        //TODO should the reward be added to new minted coin?
        // read the commitment
        // concatenate message
        // subtract those cm and commit output, constraint the output, that should equal 1.
        //
        //TODO this proof need to be for the two coins commitment,
        // but cm1 doesn't exist in public inputs, or witnesses,
        // does it make sense to calculate cm1, and proof decomm(cm1, coin2, r2)=true?
        // that doesn't make any sense!
        // this is the proof for the second commitment only
        //TODO does both coins have the same value?!! doesn't make sense
        //but only single value is in witness.

        let coin_hash = {
            let poseidon_message = [
                coin_pk_commit.inner().x(),
                coin_pk_commit.inner().y(),
                coin_value.clone(),
                coin_nonce.clone(),
            ];

            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<4>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };
        let (com, _) = {
            let coin_commit_v = ValueCommitV;
            let coin_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_commit_v);
            coin_commit_v.mul(layouter.namespace(|| "coin commit v"), (coin_hash, one.clone()))?
        };
        // r*G_2
        let (blind, _) = {
            let coin_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_commit_r);
            coin_commit_r
                .mul(layouter.namespace(|| "coin serial number commit R"), self.coin_opening_1)?
        };
        let coin_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;

        let coin_commit_x: AssignedCell<Fp, Fp> = coin_commit.inner().x();
        let coin_commit_y: AssignedCell<Fp, Fp> = coin_commit.inner().y();

        let cm1_zero_out_x =
            ar_chip.sub(layouter.namespace(|| "sub to zero"), coin_commit_x.clone(), cm_c1_x)?;
        let cm1_zero_out_y =
            ar_chip.sub(layouter.namespace(|| "sub to zero"), coin_commit_y.clone(), cm_c1_y)?;

        // constrain coin's pub key x value
        layouter.constrain_instance(
            cm1_zero_out_x.cell(),
            config.primary,
            LEAD_COIN_COMMIT_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            cm1_zero_out_y.cell(),
            config.primary,
            LEAD_COIN_COMMIT_Y_OFFSET,
        )?;

        //
        let coin2_hash = {
            let poseidon_message = [
                coin_pk_commit.inner().x(),
                coin_pk_commit.inner().y(),
                coin_value.clone(),
                coin2_nonce.inner().x(),
                coin2_nonce.inner().y(),
            ];

            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<5>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };
        let (com, _) = {
            let coin_commit_v = ValueCommitV;
            let coin_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_commit_v);
            coin_commit_v.mul(layouter.namespace(|| "coin commit v"), (coin2_hash, one.clone()))?
        };
        // r*G_2
        let (blind, _) = {
            let coin_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_commit_r);
            coin_commit_r
                .mul(layouter.namespace(|| "coin serial number commit R"), self.coin_opening_2)?
        };
        let coin2_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin2_commit_x: AssignedCell<Fp, Fp> = coin2_commit.inner().x();
        let coin2_commit_y: AssignedCell<Fp, Fp> = coin2_commit.inner().y();
        let cm2_zero_out_x =
            ar_chip.sub(layouter.namespace(|| "sub to zero"), coin2_commit_x, cm_c2_x)?;
        let cm2_zero_out_y =
            ar_chip.sub(layouter.namespace(|| "sub to zero"), coin2_commit_y, cm_c2_y)?;

        layouter.constrain_instance(
            cm2_zero_out_x.cell(),
            config.primary,
            LEAD_COIN_COMMIT2_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            cm2_zero_out_y.cell(),
            config.primary,
            LEAD_COIN_COMMIT2_X_OFFSET,
        )?;

        // ===========================
        let path: Option<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            config.merkle_chip_1(),
            config.merkle_chip_2(),
            OrchardHashDomains::MerkleCrh,
            self.cm_pos,
            path,
        );

        let coin_commit_hash: AssignedCell<Fp, Fp> = {
            let poseidon_message = [coin_commit_x.clone(), coin_commit_y.clone()];

            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<2>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };
        let computed_final_root = merkle_inputs
            .calculate_root(layouter.namespace(|| "calculate root"), coin_commit_hash)?;

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            LEAD_COIN_COMMIT_PATH_OFFSET,
        )?;

        let message = {
            let (com, _) = {
                let commit_v = ValueCommitV;
                let commit_v = FixedPointShort::from_inner(ecc_chip.clone(), commit_v);
                commit_v.mul(
                    layouter.namespace(|| "coin commit v"),
                    (coin_nonce.clone(), one.clone()),
                )?
            };
            // r*G_2
            let (blind, _) = {
                let commit_r = OrchardFixedBasesFull::ValueCommitR;
                let commit_r = FixedPoint::from_inner(ecc_chip.clone(), commit_r);
                commit_r.mul(layouter.namespace(|| "coin serial number commit R"), self.root_sk)?
            };
            com.add(layouter.namespace(|| "nonce commit"), &blind)?
        };
        let message_sum = ar_chip.add(
            layouter.namespace(|| "msg x + y"),
            message.inner().x(),
            message.inner().y(),
        )?;

        let (com, _) = {
            let y_commit_v = ValueCommitV;
            let y_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), y_commit_v);
            y_commit_v
                .mul(layouter.namespace(|| "coin commit v"), (message_sum.clone(), one.clone()))?
        };
        // r*G_2
        let (blind, _) = {
            let y_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let y_commit_r = FixedPoint::from_inner(ecc_chip.clone(), y_commit_r);
            y_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), self.mau_y)?
        };
        let mut y_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        // ============================
        //let y_commit_base  : AssignedCell<Fp,Fp> = pallas::Base::from_repr(y_commit.inner().to_bytes()).unwrap();
        //let y_commit_x  : AssignedCell<Fp,Fp> = y_commit.inner().x();
        //let y_commit_x_base   = y_commit.inner().x().value().unwrap();
        let y_commit_base_temp =
            pallas::Base::from_repr(y_commit.inner().point().unwrap().to_bytes()).unwrap();
        let y_commit_base = self.load_private(
            layouter.namespace(|| "load coin y commit as pallas::base"),
            config.advices[0],
            Some(y_commit_base_temp),
        )?;

        // ============================
        // constraint rho
        // ============================
        let (com, _) = {
            let rho_commit_v = ValueCommitV;
            let rho_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), rho_commit_v);
            rho_commit_v.mul(layouter.namespace(|| "coin commit v"), (message_sum, one.clone()))?
        };
        // r*G_2
        let (blind, _) = {
            let rho_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let rho_commit_r = FixedPoint::from_inner(ecc_chip.clone(), rho_commit_r);
            rho_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), self.mau_rho)?
        };
        let rho_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;

        //TODO in case of the v_max lead statement you need to provide a proof
        // that the coin value never get past it.

        let scalar = self.load_private(
            layouter.namespace(|| "load scalar "),
            config.advices[0],
            Some(pallas::Base::from(1024)),
        )?;
        let c = pallas::Scalar::from(3); // leadership coefficient
        let target: AssignedCell<Fp, Fp> =
            ar_chip.mul(layouter.namespace(|| "calculate target"), scalar, coin_value)?;

        eb_chip.decompose(layouter.namespace(|| "target range check"), target.clone())?;
        eb_chip.decompose(layouter.namespace(|| "y_commit  range check"), y_commit_base.clone())?;

        let (helper, is_gt) = greater_than_chip.greater_than(
            layouter.namespace(|| "t>y"),
            target.into(),
            y_commit_base.into(),
        )?; //note assuming x,y coordinates are true random each?
        eb_chip.decompose(layouter.namespace(|| "helper range check"), helper.0)?;
        layouter.constrain_instance(is_gt.0.cell(), config.primary, LEAD_THRESHOLD_OFFSET)?;
        Ok(())
    }
}
