use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointShort, ScalarFixed, ScalarFixedShort,
    },
    poseidon::{primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip, Pow5Config as PoseidonConfig},
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

use crate::crypto::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        util::gen_const_array,
        OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
    },
    merkle_node::MerkleNode,
};

use crate::zk::gadget::{
    arithmetic::{ArithChip, ArithConfig, ArithInstruction},
    even_bits::{EvenBitsChip, EvenBitsConfig, EvenBitsLookup},
};

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
    _sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    //greaterthan_config: GreaterThanConfig,
    evenbits_config: EvenBitsConfig,
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

    // fn greaterthan_chip(&self) -> GreaterThanChip<pallas::Base, WORD_BITS> {
    // GreaterThanChip::construct(self.greaterthan_config.clone())
    // }

    fn evenbits_chip(&self) -> EvenBitsChip<pallas::Base, WORD_BITS> {
        EvenBitsChip::construct(self.evenbits_config.clone())
    }

    fn arith_chip(&self) -> ArithChip {
        ArithChip::construct(self.arith_config.clone())
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

pub fn concat_u8(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    [lhs, rhs].concat()
}
#[derive(Default, Debug)]
pub struct LeadContract {
    // witness
    pub path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub coin_pk_x: Value<pallas::Base>,
    pub coin_pk_y: Value<pallas::Base>,
    pub root_sk: Value<pallas::Base>, // coins merkle tree secret key of coin1
    pub sf_root_sk: Value<pallas::Scalar>, // root_sk as pallas::Scalar
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

        // let greaterthan_config = GreaterThanChip::<pallas::Base, WORD_BITS>::configure(
        // meta,
        // advices[10..12].try_into().unwrap(),
        // primary,
        // );
        let evenbits_config = EvenBitsChip::<pallas::Base, WORD_BITS>::configure(meta);
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
            //greaterthan_config,
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
        let _ps_chip = config.poseidon_chip();
        let eb_chip = config.evenbits_chip();
        //let greater_than_chip = config.greaterthan_chip();

        eb_chip.alloc_table(&mut layouter.namespace(|| "alloc table"))?;

        // ===============
        // load witnesses
        // ===============

        let one = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Value::known(pallas::Base::one()),
        )?;

        let zero = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Value::known(pallas::Base::zero()),
        )?;

        // coin_timestamp tau

        let coin_timestamp = self.load_private(
            layouter.namespace(|| "load coin time stamp"),
            config.advices[0],
            self.coin_timestamp,
        )?;

        let coin_nonce: AssignedCell<Fp, Fp> = self.load_private(
            layouter.namespace(|| "load coin nonce"),
            config.advices[0],
            self.coin_nonce,
        )?;

        let coin_value = self.load_private(
            layouter.namespace(|| "load coin value"),
            config.advices[0],
            self.value,
        )?;
        let coin_pk_x: AssignedCell<Fp, Fp> = self.load_private(
            layouter.namespace(|| "load coin time stamp"),
            config.advices[0],
            self.coin_pk_x,
        )?;

        let coin_pk_y: AssignedCell<Fp, Fp> = self.load_private(
            layouter.namespace(|| "load coin time stamp"),
            config.advices[0],
            self.coin_pk_y,
        )?;

        let _slot = self.load_private(layouter.namespace(|| ""), config.advices[0], self.slot)?;

        let _root_sk =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.root_sk)?;

        // ===============
        // coin 2 nonce
        // ===============
        // m*G_1
        let (com, _) = {
            let nonce2_commit_v = ValueCommitV;
            let nonce2_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), nonce2_commit_v);
            let coin_nonce = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin_nonce*1"),
                (coin_nonce.clone(), one.clone()),
            )?;
            nonce2_commit_v.mul(layouter.namespace(|| "coin_pk commit v"), coin_nonce)?
        };
        // r*G_2
        let (blind, _) = {
            let nonce2_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let nonce2_commit_r = FixedPoint::from_inner(ecc_chip.clone(), nonce2_commit_r);
            let root_sk = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "root_sk scalar"),
                self.sf_root_sk,
            )?;
            nonce2_commit_r.mul(layouter.namespace(|| "nonce2 commit R"), root_sk)?
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
            let coin_timestamp = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin_timestamp*1"),
                (coin_timestamp, one.clone()),
            )?;
            coin_pk_commit_v.mul(layouter.namespace(|| "coin_pk commit v"), coin_timestamp)?
        };
        // r*G_2
        let (blind, _) = {
            let coin_pk_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_pk_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_pk_commit_r);
            let root_sk = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "root sk scalar"),
                self.sf_root_sk,
            )?;
            coin_pk_commit_r.mul(layouter.namespace(|| "coin_pk commit R"), root_sk)?
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
            let coin_nonce = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin nonce * 1"),
                (coin_nonce.clone(), one.clone()),
            )?;

            sn_commit_v.mul(layouter.namespace(|| "coin serial number commit v"), coin_nonce)?
        };
        // r*G_2
        let (blind, _) = {
            let sn_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let sn_commit_r = FixedPoint::from_inner(ecc_chip.clone(), sn_commit_r);
            let root_sk = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "root sk scalar"),
                self.sf_root_sk,
            )?;
            sn_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), root_sk)?
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
        //TODO (res) should the reward be added to new minted coin?
        /*
        let coin_val = {
        let coin_val_pt = ar_chip.mul(layouter.namespace(|| ""), &coin_pk_y, &coin_pk_x)?;

        //TODO (FIX)
        //let coin_val0 = ar_chip.mul(layouter.namespace(|| ""), &coin_nonce, &coin_value)?;
        let coin_val0 = ar_chip.mul(layouter.namespace(|| ""), &one.clone(), &zero.clone())?;
        ar_chip.mul(layouter.namespace(|| ""), &coin_val_pt, &coin_val0)?
    };
         */


        let coin_hash : AssignedCell<Fp,Fp> = {
            let poseidon_message = [
                //TODO (fix)
                //coin_pk_x.clone(),
                //coin_pk_y.clone(),
                //coin_value.clone(),
                //coin_nonce.clone(),  //TODO (fix) nocne need to be for (x,y), or if the seed is changed to hash it can be just pallas
                one.clone(),
            ];

            let poseidon_hasher = PoseidonHash::<_, _, poseidon::P128Pow5T3, poseidon::ConstantLength<1>, 3, 2>::init(
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
            //TODO (FIX) THIS PANICS, ONLY PANICS ON LARGE VALUES!
            /*
            let coin_hash_pt = ScalarFixedShort::new(
            ecc_chip.clone(),
            layouter.namespace(|| "coin_val*1"),
            (coin_hash, one.clone()),
            )?;
             */
            //note c_v is set to zero, should work


            let coin_hash_pt = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin_val*1"),
                (coin_value.clone(), one.clone()),
            )?;
            coin_commit_v.mul(layouter.namespace(|| "coin commit v"), coin_hash_pt)?
        };

        // r*G_2
        let (blind, _) = {
            let coin_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_commit_r);
            let rcv = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin1 blind scalar"),
                self.coin1_blind,
            )?;
            coin_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), rcv)?
        };

        let coin_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;

        let coin_commit_x: AssignedCell<Fp, Fp> = coin_commit.inner().x();
        let coin_commit_y: AssignedCell<Fp, Fp> = coin_commit.inner().y();

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

        //
        /*
        let coin2_hash = {
            let poseidon_message = [
                //coin_pk_commit.inner().x(),
                //coin_pk_commit.inner().y(),
                //coin_value.clone(),
                //coin2_nonce.inner().x(),
                //coin2_nonce.inner().y(),
                zero.clone(),
            ];

            let poseidon_hasher = PoseidonHash::<_, _, poseidon::P128Pow5T3, poseidon::ConstantLength<5>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };
        /*
        let coin2_hash0 = ar_chip.mul(
            layouter.namespace(|| ""),
            &coin_pk_commit.inner().x(),
            &coin_pk_commit.inner().y(),
        )?;
        let coin2_hash1 = ar_chip.mul(
            layouter.namespace(|| ""),
            &coin2_nonce.inner().x(),
            &coin2_nonce.inner().y(),
        )?;
        let coin2_hash2 = ar_chip.mul(layouter.namespace(|| ""), &coin2_hash0, &coin2_hash1)?;

        //TODO (fix)
        //let coin2_hash = ar_chip.mul(layouter.namespace(|| ""), &coin_value.clone(), &coin2_hash2)?;
        let coin2_hash = ar_chip.mul(layouter.namespace(|| ""), &one.clone(), &zero.clone())?;
        */
        let (com, _) = {
            let coin_commit_v = ValueCommitV;
            let coin_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_commit_v);
            let coin2_hash_pt = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin2_hash*1"),
                (coin2_hash, one.clone()),
            )?;
            coin_commit_v.mul(layouter.namespace(|| "coin commit v"), coin2_hash_pt)?
        };
        // r*G_2
        let (blind, _) = {
            let coin_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_commit_r);
            let coin2_blind = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "coin2 blind scalar"),
                self.coin2_blind,
            )?;
            coin_commit_r
                .mul(layouter.namespace(|| "coin serial number commit R"), coin2_blind)?
        };
        let coin2_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let coin2_commit_x: AssignedCell<Fp, Fp> = coin2_commit.inner().x();
        let coin2_commit_y: AssignedCell<Fp, Fp> = coin2_commit.inner().y();

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

        // ===========================
        let path : Value<[pallas::Base;MERKLE_DEPTH_ORCHARD]> = self.path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

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

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            LEAD_COIN_COMMIT_PATH_OFFSET,
        )?;

        //let _node = MerkleNode::from_bytes(&self.root_sk.unwrap().to_repr()).unwrap();
        //let serialized = serde_json::to_string(&node).unwrap();
        //println!("root_sk: {}", serialized);

        //TODO (research) this multiplication panics!
        let y_commit_exp = ar_chip.mul(
            layouter.namespace(|| ""),
            &coin_nonce,
            //root_sk.clone(), //(fix)
            &one,
        )?;

        let (com, _) = {
            let y_commit_v = ValueCommitV;
            let y_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), y_commit_v);
            let y_commit_exp = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "y_commit_exp*1"),
                (y_commit_exp, one.clone()),
            )?;
            y_commit_v.mul(layouter.namespace(|| "coin commit v"), y_commit_exp)?
        };

        // r*G_2
        let (blind, _) = {
            let y_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let y_commit_r = FixedPoint::from_inner(ecc_chip.clone(), y_commit_r);
            let mau_y = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "mau_y scalar"),
                self.mau_y,
            )?;
            y_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), mau_y)?
        };
        let y_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        let y_commit_base = y_commit.inner().x();

        // ============================
        //let _y_commit_bytes : [u8;32] = y_commit.inner().point().inner().unwrap().to_repr();

        //TODO (fix) temp abuse until even_bits is fixed.
        let zero_base_bytes : [u8;32] = [0;32];
        let y_commit_base_temp = pallas::Base::from_repr(zero_base_bytes).unwrap();
        let y_commit_base = self.load_private(
            layouter.namespace(|| "load coin y commit as pallas::base"),
            config.advices[0],
            Value::known(y_commit_base_temp),
        )?;
        /*
        let y_commit_base_bytes: [u8; 32] = [0; 32];
        //note! due to 24bytes size limitation in the comparision gate we need first 24bytes
        for i in 0..23 {
            y_commit_base_bytes[i] = y_commit_base_bytes[i];
        }
        let y_commit_base_temp = pallas::Base::from_repr(y_commit_base_bytes).unwrap();

        let y_commit_base = self.load_private(
            layouter.namespace(|| "load coin y commit as pallas::base"),
            config.advices[0],
            Value::known(y_commit_base_temp),
        )?;
        */
        // ============================
        // constraint rho
        // ============================
        let (com, _) = {
            let rho_commit_v = ValueCommitV;
            let rho_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), rho_commit_v);
            let y_commit_base = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "y_commit_base*1"),
                (y_commit_base.clone(), one.clone()),
            )?;
            rho_commit_v.mul(layouter.namespace(|| "coin commit v"), y_commit_base)?
        };
        // r*G_2
        let (blind, _) = {
            let rho_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let rho_commit_r = FixedPoint::from_inner(ecc_chip.clone(), rho_commit_r);
            let mau_rho =
                ScalarFixed::new(ecc_chip, layouter.namespace(|| "mau_rho scalar"), self.mau_rho)?;
            rho_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), mau_rho)?
        };
        let _rho_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;

        // that the coin value never get past it.
        let scalar = self.load_private(
            layouter.namespace(|| "load scalar "),
            config.advices[0],
            Value::known(pallas::Base::from(1024)),
        )?;
        //leadership coefficient
        let c = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            Value::known(pallas::Base::one()), // note! this parameter to be tuned.
        )?;
        let ord = ar_chip.mul(layouter.namespace(|| ""), &scalar, &c)?;
        //TODO (fix)
        //let target = ar_chip.mul(layouter.namespace(|| "calculate target"), &ord, &coin_value.clone())?;
        let target = ar_chip.mul(layouter.namespace(|| "calculate target"), &one.clone(), &zero.clone())?;
        eb_chip.decompose(layouter.namespace(|| "target range check"), target.clone())?;
        eb_chip.decompose(layouter.namespace(|| "y_commit  range check"), y_commit_base.clone())?;
        */
        //let (helper, is_gt) = greater_than_chip.greater_than(
        //  layouter.namespace(|| "t>y"),
        //target.into(),
        //            y_commit_base.into(),
        //      )?;
        //eb_chip.decompose(layouter.namespace(|| "helper range check"), helper.0)?;

        //layouter.constrain_instance(is_gt.0.cell(), config.primary, LEAD_THRESHOLD_OFFSET)?

        Ok(())
    }
}
