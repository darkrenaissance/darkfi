use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointShort,NonIdentityPoint,Point
    },
    poseidon::{Hash as PoseidonHash, Pow5Chip as PoseidonChip, Pow5Config as PoseidonConfig},
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    utilities::{lookup_range_check::LookupRangeCheckConfig, UtilitiesInstructions},
};

use halo2_proofs::{
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};

use pasta_curves::{pallas, Fp};

use crate::crypto::constants::{
    constants::{
        util::gen_const_array,
    },
    OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV,
};

use create::zk::{
    arith_chip::{ArithmeticChipConfig, ArithmeticChip},
    GreaterThanChip, GreatherThanConfig,
};

#[derive(Clone,Debug)]
pub struct LeadConfig
{
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>;19],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base,3,2>,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_2: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    greaterthan_config: GreaterThanConfig,
    arith_config: ArithmeticChipConfig,
}

//TODO what is the pederson commitment pallas order number of bits?
const COMMIT_GROUP_ORDER_BITS : usize = 264;

impl LeadConfig
{
    fn ecc_chip(&self) -> EccChip<OrchardFixedBass>
    {
        EccChip::construct(self.ecc_config.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2>
    {
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

    fn arith_chip(&self) -> ArithmeticChip {
        ArithmeticChip::construct(self.arith_config.clone())
    }
}


//
const LEAD_COIN_PK_X_OFFSET: usize = 0;
const LEAD_COIN_PK_y_OFFSET: usize = 1;
//
const LEAD_COIN_NONCE2_X_OFFSET: usize = 2;
const LEAD_COIN_NONCE2_y_OFFSET: usize = 3;

const LEAD_COIN_SERIAL_NUMBER_X_OFFSET: usize = 4;
const LEAD_COIN_SERIAL_NUMBER_Y_OFFSET: usize = 5;
//

const LEAD_COIN2_SERIAL_NUMBER_X_OFFSET: usize = 8;
const LEAD_COIN2_SERIAL_NUMBER_Y_OFFSET: usize = 9;
//
const LEAD_COIN_COMMIT_PATH_OFFSET: usize = 6;

const LEAD_LEAD_THRESHOLD_OFFSET: usize = 7;

#[derive(Debug,Default)]
pub struct LeadContract
{
    // witness
    pub path : Option<[MerkleNode;MERKLE_DEPTH_ORCHARD]>,
    pub root_sk : Option<u32>, // coins merkle tree secret key of coin1
    pub path_sk : Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the secret key root_sk
    pub coin_timestamp: Option<u32>,
    pub coin_nonce : Option<u32>,
    pub coin_opening_1 :Option<pallas::Base>,
    pub value: Option<u32>,
    pub coin_opening_2 :Option<pallas::Base>,
    // public advices
    pub cm_c2 : Option<NonIdentityPoint>,
    pub sn_c1 : Option<u32>,
    pub eta : Option<u32>, //TODO name, type
    pub slot : Option<u32>,
    pub rho : Option<u32>, //TODO name, type
    pub h : Option<u32>, // hash of this data
    pub ptr: Option<u32>, //hash of the previous block
    pub mau_rho: Option<u32>, //TODO name, type
    pub mau_y: Option<u32>, //TODO name, type
    pub root : Option<u32>, //TODO name, type
}

impl UtilitiesInstruction<pallas::Base> for LeadContract {
    type var = AssignedCell<Fp, Fp>;
}

impl circuit<pallas::Base> for LeadContract {
    type Config = LeadConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witness(&self) -> Self {
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
            meta.fixed_column(),
        ];

        let rc_a = lagrange_coeffs[2..5].try_into().unwrap();
        let rc_b = lagrange_coeffs[5..8].try_into().unwrap();

        meta.enable_constant(laggrange_coeffs[0]);
        let range_check = LookupRangeCheckConfig::configure(meta, advices[8], table_idx);


        //TODO how many columns needed for the eccChip?
        //i assumed 5 for constants/private_witnesses
        let ecc_config = EccChip::<OrchardFixedBases>::configure(meta, advices, lagrange_coeffs, range_check);

        let poseidon_config = PoseidonChip::configure<P128Pow5T3>(
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
                advices[5.9].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2.clone());

            (sinsemilla_config_2, merkle_config_2)
        };

        let  greaterthan_config = GreaterThanChip::<pallas::Base>::configure(meta, advices[10..11]);
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
            arith_config,
        }
    }

    fn synthesize(&self,
                  config: Self::Config,
                  mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(),Error> {
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;
        let ecc_chip = config.ecc_chip();
        let ar_chip = config.arith_chip();

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
        let coin_root = self.load_private(
            layouter.namespace(|| "load root coin"),
            config.advices[0],
            self.root_sk,
        )?;

        // coin nonce
        let coin_nonce = self.load_private(
            layouter.namespace(|| "load coin nonce"),
            config.advices[0],
            self.nonce,
        )?;

        let coin_opening_1 = self.load_private(
            layouter.namespace(|| "load opening 1"),
            config.advices[0],
            self.coin_opening_1,
        )?;

        let coin_value = self.load_private(
            layouter.namespace(|| "load opening 1"),
            config.advices[0],
            self.value,
        )?;

        let coin_opening_2 = self.load_private(
            layouter.namespace(|| "load opening 2"),
            config.advices[0],
            self.coin_opening_2,
        )?;

        let cm_c2 = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.cm_c2,
        )?;

        let sn_c1 = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.sn_c1,
        )?;

        let eta = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.eta,
        )?;

        let slot = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.slot,
        )?;

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

        let root = self.load_private(
            layouter.namespace(|| ""),
            config.advices[0],
            self.root,
        )?;

        //TODO read the second coin commitment as constant(public input)
        // in this case
        //

        // ===============
        // coin 2 nonce
        // ===============
        // m*G_1
        let (com, _ )  = {
            let nonce2_commit_v = ValueCommitV;
            let nonce2_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), nonce2_commit_v);
            nonce2_commit_v.mul(layouter.namespace(|| "coin_pk commit v"), (coin_nonce, one))?
        };
        // r*G_2
        let (blind, _) = {
            let nonce2_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let nonce2_commit_r = FixedPoint::from_inner(ecc_chip.clone(), nonce2_commit_r);
            nonce2_commit_r.mul(layouter.namespace(|| "nonce2 commit R"), (coin_root.clone(), one))?
        };
        let coin2_nonce = com.add(layouter.namespace(|| "nonce2 commit"), &blind)?;
        layouter.constrain_instance(
            coin2_nonce.inner().x().cell(),
            config.primary,
            LEAD_COIN_PK_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin2_nonce.inner().y().cell(),
            coinfig.primary,
            CLEAD_COIN_PK_Y_OFFSET,
        )?;
        // ================
        // coin public key constraints derived from the coin timestamp
        // ================

        // m*G_1
        let (com, _ )  = {
            let coin_pk_commit_v = ValueCommitV;
            let coin_pk_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_pk_commit_v);
            coin_timestamp_commit_v.mul(layouter.namespace(|| "coin_pk commit v"), (coin_timestamp, one))?
        };
        // r*G_2
        let (blind, _) = {
            let coin_pk_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let coin_pk_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_pk_commit_r);
            coin_timestamp_commit_r.mul(layouter.namespace(|| "coin_pk commit R"), (coin_root.clone(), one))?
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
            coinfig.primary,
            CLEAD_COIN_PK_Y_OFFSET,
        )?;

        // =================
        // nonce constraints derived from previous coin's nonce
        // =================


        // =============
        // constrain coin c1 serial number
        // =============
        // m*G_1
        let (com, _ )  = {
            let sn_commit_v = ValueCommitV;
            let sn_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), sn_commit_v);
            sn_commit_v.mul(layouter.namespace(|| "coin serial number commit v"), (coin_nonce, one))?
        };
        // r*G_2
        let (blind, _) = {
            let sn_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let sn_commit_r = FixedPoint::from_inner(ecc_chip.clone(), sn_commit_r);
            sn_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), (coin_root.clone(), one))?
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
            nonce_commit.inner().y().cell(),
            coinfig.primary,
            LEAD_COIN_SERIAL_NUMBER_X_OFFSET,
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
        //let coin_tup = [coin_pk_commit, coin_value, coin2_nonce];
        //TODO this should be the concat of coin_tup
        let coin_ = ar_chip.add(layouter.namespace(|| ""), (coin_pk_commit, coin_value))?;
        let coin = ar_chip.add(layouter.namespace(|| ""), (coin_, coin_nonce))?;
        let (com, _ )  = {
            let coin_commit_v = ValueCommitV;
            let coin_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_commit_v);
            sn_commit_v.mul(layouter.namespace(|| "coin commit v"), (coin, one))?
        };
        // r*G_2
        let (blind, _) = {
            let sn_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let sn_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_opening_1);
            sn_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), (coin_root, one))?
        };
        let coin_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        //TODO it would be better if you coin1_commit +  -1*cm_c2 and constraint 0 (output)
        // constrain coin's pub key x value
        layouter.constrain_instance(
            coin_commit.inner().x().cell(),
            config.primary,
            LEAD_COIN_SERIAL_NUMBER_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin_commit.inner().y().cell(),
            config.primary,
            LEAD_COIN_SERIAL_NUMBER_X_OFFSET,
        )?;
        //=========================
        let coin = ar_chip.add(layouter.namespace(|| ""), (coin_, coin2_nonce))?;
        let (com, _ )  = {
            let coin_commit_v = ValueCommitV;
            let coin_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), coin_commit_v);
            sn_commit_v.mul(layouter.namespace(|| "coin commit v"), (coin, one))?
        };
        // r*G_2
        let (blind, _) = {
            let r = self.root_sk;
            let sn_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let sn_commit_r = FixedPoint::from_inner(ecc_chip.clone(), coin_opening_2);
            sn_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), r)?
        };
        let coin2_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        layouter.constrain_instance(
            coin2_commit.inner().x().cell(),
            config.primary,
            LEAD_COIN2_SERIAL_NUMBER_X_OFFSET,
        )?;
        // constrain coin's pub key y value
        layouter.constrain_instance(
            coin2_commit.inner().y().cell(),
            config.primary,
            LEAD_COIN2_SERIAL_NUMBER_X_OFFSET,
        )?;
        //TODO you need to add this coin1_commit to the tree and return rooted by self.root,
        // and return the position coin1_commit_pos
        let coin1_commit_pos : u32 = 0;
        // ===========================
        let path: Option<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            config.merkle_chip_1(),
            config.merkle_chip_2(),
            OrchardHashDomains::MerkleCrh,
            coin1_commit_pos,
            path,
        );

        let computed_final_root =
            merkle_inputs.calculate_root(layouter.namespace(|| "calculate root"), coin1_commit)?;

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            LEAD_COIN_COMMIT_PATH_OFFSET,
        )?;
        // =============================
        /*
        let path: Option<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.path_sk.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            config.merkle_chip_1(),
            config.merkle_chip_2(),
            OrchardHashDomains::MerkleCrh,
            self.slot - self.coin_timestamp,
            path,
        );

        //TODO fix this is a path to a leaf, i have no clue of that leaf
        let computed_final_root =
            merkle_inputs.calculate_root(layouter.namespace(|| "calculate root"), coin1_commit)?;

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            LEAD_COIN_COMMIT_PATH_OFFSET,
        )?;
         */
        // ============================
        // constrain y
        //
        let message = ar_chip.add(layouter.namespace(|| ""), root_sk, coin_nonce)?;
        let (com, _ )  = {
            //TODO concatenate message
            //TODO message need to be eccchip::var
            //let message = [root_sk, coin_nonce];
            let y_commit_v = ValueCommitV;
            let y_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), y_commit_v);
            y_commit_v.mul(layouter.namespace(|| "coin commit v"), (message.clone(), one))?
        };
        // r*G_2
        let (blind, _) = {
            //let r = mau_y;
            let y_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let y_commit_r = FixedPoint::from_inner(ecc_chip.clone(), y_commit_r);
            y_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), (mau_y, one))?
        };
        let y_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;
        // ============================
        // constraint rho
        //
        let (com, _ )  = {
            //TODO concatenate message
            //TODO message need to be eccchip::var
            //let message = [root_sk, coin_nonce];
            let y_commit_v = ValueCommitV;
            let y_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), y_commit_v);
            y_commit_v.mul(layouter.namespace(|| "coin commit v"), (message.clone(), one))?
        };
        // r*G_2
        let (blind, _) = {
            let r = mau_rho;
            let y_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let y_commit_r = FixedPoint::from_inner(ecc_chip.clone(), y_commit_r);
            y_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), r)?
        };
        let rho_commit = com.add(layouter.namespace(|| "nonce commit"), &blind)?;

        // ===========================
        // lead statment
        // ===========================
        let scalar = pallas::Scalar::from(1024);
        let c = pallas::Scalar::from(3); // leadership coefficient
        let target = ar_chip.mul(layouter.namespace(|| "calculate target"), scalar, value)?;

        let greater_than_chip = config.greaterthan_config();
        greater_than_chip.greater_than(layouter.namespace("t>y"), target , y_commit);
        layouter.constrain_instance(
            greater_than_chip.cell(),
            config.primary,
            LEAD_LEAD_THRESHOLD_OFFSET,
        )?;
    }
}
