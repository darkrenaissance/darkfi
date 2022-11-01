/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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
        FixedPoint, FixedPointBaseField, NonIdentityPoint, ScalarFixed,
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
    pasta::{group::Curve, pallas, Fp},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};

use crate::zk::gadget::{
    arithmetic::{ArithChip, ArithConfig, ArithInstruction},
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

const LEAD_COIN_NONCE2_OFFSET: usize = 0;
const LEAD_COIN_PK_X_OFFSET: usize = 1;
const LEAD_COIN_PK_Y_OFFSET: usize = 2;
const LEAD_Y_COMMIT_BASE_OFFSET: usize = 3;

#[derive(Default, Debug)]
pub struct LeadContract {
    pub path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    /// path of coin1 commitment
    pub sk: Value<pallas::Base>,
    /// coin secret key
    pub root_sk: Value<pallas::Base>,
    /// root to secret key
    pub path_sk: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    /// path to the secret key rooted by `root_sk`
    pub coin_timestamp: Value<pallas::Base>,
    /// $\tau$ or coin index
    pub coin_nonce: Value<pallas::Base>,
    /// coin nonce $\rho_{c1}$
    pub coin1_blind: Value<pallas::Scalar>,
    /// coin blind r
    pub coin1_sn: Value<pallas::Base>,
    /// coin serial number
    pub value: Value<pallas::Base>,
    /// coin value
    pub coin2_blind: Value<pallas::Scalar>,
    /// coin2 blind r2
    pub coin2_commit: Value<pallas::Point>,
    /// coin2 commitment
    pub cm_pos: Value<u32>,
    /// coin position in the merkle tree
    pub slot: Value<pallas::Base>,
    /// slot index
    pub mau_rho: Value<pallas::Scalar>,
    /// random value derived from eta used for constraining \rho
    pub mau_y: Value<pallas::Scalar>,
    /// random value derived from eta used for calculating y
    pub root_cm: Value<pallas::Base>,
    /// root to commitment in the mekle tree
    pub sigma1: Value<pallas::Base>,
    /// first coefficient in 1-term T (target function) approximation
    pub sigma2: Value<pallas::Base>,
    /// second coefficient in 2-term T (target function) approximation
    /// constrained nonce $\rho$
    pub rho: Value<pallas::Point>,
    //pub eta : Option<u32>,
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

            let z1 = meta.advice_column();
            let z2 = meta.advice_column();

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            LessThanChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::configure(
                meta,
                a,
                b,
                a_offset,
                z1,
                z2,
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

        // prefix to the pseudo-random-function that prefix input
        // to the nullifier poseidon hash
        let prf_nullifier_prefix_base = self.load_private(
            layouter.namespace(|| "PRF NULLIFIER PREFIX BASE"),
            config.advices[0],
            Value::known(pallas::Base::from(PRF_NULLIFIER_PREFIX)),
        )?;
        // staking coin nonce
        let coin_nonce: AssignedCell<Fp, Fp> = self.load_private(
            layouter.namespace(|| "load coin nonce"),
            config.advices[0],
            self.coin_nonce,
        )?;

        let coin1_sn: AssignedCell<Fp, Fp> = self.load_private(
            layouter.namespace(|| "load coin1 sn"),
            config.advices[0],
            self.coin1_sn,
        )?;

        // staking coin value
        let coin_value = self.load_private(
            layouter.namespace(|| "load coin value"),
            config.advices[0],
            self.value,
        )?;

        // staking coin secret key
        let _root_sk =
            self.load_private(layouter.namespace(|| "root sk"), config.advices[0], self.root_sk)?;

        let root_cm =
            self.load_private(layouter.namespace(|| ""), config.advices[0], self.root_cm)?;

        // staking coin secret key
        let sk: AssignedCell<Fp, Fp> =
            self.load_private(layouter.namespace(|| "sk"), config.advices[0], self.sk)?;

        let sigma1 = self.load_private(
            layouter.namespace(|| "load sigma1 "),
            config.advices[0],
            self.sigma1,
        )?;

        let sigma2 = self.load_private(
            layouter.namespace(|| "load sigma2 "),
            config.advices[0],
            self.sigma2,
        )?;

        let one = self.load_private(
            layouter.namespace(|| "one"),
            config.advices[0],
            Value::known(pallas::Base::one()),
        )?;

        // the original crypsinous coin pk is as follows.
        // coin public key pk=PRF_{root_sk}(tau)
        // coin public key is pseudo random hash of concatenation of the following:
        // coin timestamp, and root of coin's secret key.
        // staking coin timestamp
        //let coin_timestamp = self.load_private(
        //layouter.namespace(|| "load coin time stamp"),
        //config.advices[0],
        //self.coin_timestamp,
        //)?;
        //let coin_pk_commit: AssignedCell<Fp, Fp> = {
        //    let poseidon_message = [coin_timestamp, _root_sk.clone()];
        //  //let poseidon_hasher = PoseidonHash::<
        //      _,
        //      _,
        //      poseidon::P128Pow5T3,
        //      poseidon::ConstantLength<2>,
        //      3,
        //      2,
        //  >::init(
        //      config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
        //  )?;
        //
        //  let poseidon_output =
        //      poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;
        //  let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
        //  poseidon_output
        //};
        // darkfi coin pk is based off secret key:
        // pk = G(nullifierK) * sk
        // the later is implemented for the sake of conversion between
        // lead coin  and owncoin.
        let coin_pk = {
            let coin_pk_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            coin_pk_commit_v.mul(layouter.namespace(|| "coin pk commit v"), sk)?
        };
        let coin_pk_x = coin_pk.inner().x();
        let coin_pk_y = coin_pk.inner().y();
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
                    coin_pk_x.clone(),
                    coin_pk_y.clone(),
                    coin_value.clone(),
                    coin_nonce.clone(),
                    one.clone(),
                ];
                let poseidon_hasher = PoseidonHash::<
                    _,
                    _,
                    poseidon::P128Pow5T3,
                    poseidon::ConstantLength<6>,
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

        // nonce2  =  PRF_{root_sk}(coin_nonce)
        // poured coin nonce as a poseidon of the previous nonce, and
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
                    prf_nullifier_prefix_base,
                    coin_pk_x.clone(),
                    coin_pk_y.clone(),
                    coin_value.clone(),
                    coin2_nonce.clone(),
                    one.clone(),
                ];
                let poseidon_hasher = PoseidonHash::<
                    _,
                    _,
                    poseidon::P128Pow5T3,
                    poseidon::ConstantLength<6>,
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

        // path is valid path to staked coin's commitment
        let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.cm_pos,
            path,
        );

        //TODO (fix) replace mul by hash
        let coin_commit_prod: AssignedCell<Fp, Fp> = {
            let coin_commit_coordinates = coin_commit.inner();

            let res: AssignedCell<Fp, Fp> = ar_chip.mul(
                layouter.namespace(|| ""),
                &coin_commit_coordinates.x(),
                &coin_commit_coordinates.y(),
            )?;
            res
        };

        let coin_cm_root = merkle_inputs
            .calculate_root(layouter.namespace(|| "calculate root"), coin_commit_prod)?;
        // lhs of the leader election lottery
        // *  y as COMIT(root_sk||nonce, mau_y)
        // beging the commitment to the coin's secret key, coin's nonce, and
        // random value deriven from the epoch sampled random eta.
        let lottery_commit_msg: AssignedCell<Fp, Fp> = {
            let poseidon_message = [_root_sk, coin_nonce];
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
        let y_commit_base: AssignedCell<Fp, Fp> = {
            let y_commit_base_x = y_commit.inner().x();
            let y_commit_base_y = y_commit.inner().y();
            let y_coord = [y_commit_base_x, y_commit_base_y];
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
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), y_coord)?;
            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };
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
        let rho = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness rho"),
            self.rho.map(|x| x.to_affine()),
        )?;
        let term1 =
            ar_chip.mul(layouter.namespace(|| "calculate term1"), &sigma1, &coin_value.clone())?;

        let term2_1 = ar_chip.mul(
            layouter.namespace(|| "calculate term2_1"),
            &sigma2,
            &coin_value.clone(),
        )?;

        let term2 =
            ar_chip.mul(layouter.namespace(|| "calculate term2"), &term2_1, &coin_value.clone())?;

        let target = ar_chip.add(layouter.namespace(|| "calculate target"), &term1, &term2)?;
        let target: Value<pallas::Base> = target.value().cloned();

        let y: Value<pallas::Base> = y_commit_base.value().cloned();

        less_than_chip.witness_less_than(
            layouter.namespace(|| "y < target"),
            y,
            target,
            0,
            true,
        )?;

        layouter.constrain_instance(coin2_nonce.cell(), config.primary, LEAD_COIN_NONCE2_OFFSET)?;

        layouter.constrain_instance(coin_pk_x.cell(), config.primary, LEAD_COIN_PK_X_OFFSET)?;
        layouter.constrain_instance(coin_pk_y.cell(), config.primary, LEAD_COIN_PK_Y_OFFSET)?;

        layouter.constrain_instance(
            y_commit_base.cell(),
            config.primary,
            LEAD_Y_COMMIT_BASE_OFFSET,
        )?;

        rho_commit.constrain_equal(layouter.namespace(|| ""), &rho)?;
        let ref_coin2_cm = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness coin2 cm"),
            self.coin2_commit.map(|x| x.to_affine()),
        )?;
        coin2_commit.constrain_equal(layouter.namespace(|| ""), &ref_coin2_cm)?;
        layouter.assign_region(
            || "",
            |mut region| {
                region.constrain_equal(sn_commit.cell(), coin1_sn.cell())?;
                region.constrain_equal(coin_cm_root.cell(), root_cm.cell())
            },
        )?;

        Ok(())
    }
}
