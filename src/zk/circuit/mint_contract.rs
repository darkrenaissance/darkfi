use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointShort, ScalarFixed, ScalarFixedShort,
    },
    poseidon::{
        primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
        Pow5Config as PoseidonConfig,
    },
    utilities::{lookup_range_check::LookupRangeCheckConfig, UtilitiesInstructions},
};
use halo2_proofs::{
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use pasta_curves::{pallas, Fp};

use crate::crypto::constants::{OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV};

#[derive(Clone, Debug)]
pub struct MintConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
}

impl MintConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

// The public input array offsets
const MINT_COIN_OFFSET: usize = 0;
const MINT_VALCOMX_OFFSET: usize = 1;
const MINT_VALCOMY_OFFSET: usize = 2;
const MINT_TOKCOMX_OFFSET: usize = 3;
const MINT_TOKCOMY_OFFSET: usize = 4;

#[derive(Default, Debug)]
pub struct MintContract {
    pub pub_x: Option<pallas::Base>,         // x coordinate for pubkey
    pub pub_y: Option<pallas::Base>,         // y coordinate for pubkey
    pub value: Option<pallas::Base>,         // The value of this coin
    pub token: Option<pallas::Base>,         // The token ID
    pub serial: Option<pallas::Base>,        // Unique serial number corresponding to this coin
    pub coin_blind: Option<pallas::Base>,    // Random blinding factor for coin
    pub value_blind: Option<pallas::Scalar>, // Random blinding factor for value commitment
    pub token_blind: Option<pallas::Scalar>, // Random blinding factor for the token ID
}

impl UtilitiesInstructions<pallas::Base> for MintContract {
    type Var = AssignedCell<pallas::Base, pallas::Base>;
}

impl Circuit<pallas::Base> for MintContract {
    type Config = MintConfig;
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

        let table_idx = meta.lookup_table_column();

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
        let ecc_config =
            EccChip::<OrchardFixedBases>::configure(meta, advices, lagrange_coeffs, range_check);

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
            meta,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        MintConfig { primary, advices, ecc_config, poseidon_config }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        let ecc_chip = config.ecc_chip();

        let pub_x = self.load_private(
            layouter.namespace(|| "load pubkey x"),
            config.advices[0],
            self.pub_x,
        )?;

        let pub_y = self.load_private(
            layouter.namespace(|| "load pubkey y"),
            config.advices[0],
            self.pub_y,
        )?;

        let value =
            self.load_private(layouter.namespace(|| "load value"), config.advices[0], self.value)?;

        let token =
            self.load_private(layouter.namespace(|| "load token"), config.advices[0], self.token)?;

        let serial = self.load_private(
            layouter.namespace(|| "load serial"),
            config.advices[0],
            self.serial,
        )?;

        let coin_blind = self.load_private(
            layouter.namespace(|| "load coin_blind"),
            config.advices[0],
            self.coin_blind,
        )?;

        // =========
        // Coin hash
        // =========
        let coin = {
            let poseidon_message = [pub_x, pub_y, value.clone(), token.clone(), serial, coin_blind];

            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<6>,
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

        // Constrain the coin C
        layouter.constrain_instance(coin.cell(), config.primary, MINT_COIN_OFFSET)?;

        // ================
        // Value commitment
        // ================

        // This constant one is used for short multiplication
        let one = self.load_private(
            layouter.namespace(|| "load constant one"),
            config.advices[0],
            Some(pallas::Base::one()),
        )?;

        // v * G_1
        let (commitment, _) = {
            let value_commit_v = ValueCommitV;
            let value_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), value_commit_v);
            let value = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "value"),
                (value, one.clone()),
            )?;
            value_commit_v.mul(layouter.namespace(|| "[value] ValueCommitV"), value)?
        };

        // r_V * G_2
        let (blind, _rcv) = {
            let rcv = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "value_blind"),
                self.value_blind,
            )?;
            let value_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let value_commit_r = FixedPoint::from_inner(ecc_chip.clone(), value_commit_r);
            value_commit_r.mul(layouter.namespace(|| "[value_blind] ValueCommitR"), rcv)?
        };

        let value_commit = commitment.add(layouter.namespace(|| "valuecommit"), &blind)?;

        // Constrain the value commitment coordinates
        layouter.constrain_instance(
            value_commit.inner().x().cell(),
            config.primary,
            MINT_VALCOMX_OFFSET,
        )?;

        layouter.constrain_instance(
            value_commit.inner().y().cell(),
            config.primary,
            MINT_VALCOMY_OFFSET,
        )?;

        // ================
        // Token commitment
        // ================
        // a * G_1
        let (commitment, _) = {
            let token_commit_v = ValueCommitV;
            let token_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), token_commit_v);
            let token = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "token"),
                (token, one),
            )?;
            token_commit_v.mul(layouter.namespace(|| "[token] ValueCommitV"), token)?
        };

        // r_A * G_2
        let (blind, _rca) = {
            let rca = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "token_blind"),
                self.token_blind,
            )?;
            let token_commit_r = OrchardFixedBasesFull::ValueCommitR;
            let token_commit_r = FixedPoint::from_inner(ecc_chip, token_commit_r);
            token_commit_r.mul(layouter.namespace(|| "[token_blind] ValueCommitR"), rca)?
        };

        let token_commit = commitment.add(layouter.namespace(|| "tokencommit"), &blind)?;

        // Constrain the token commitment coordinates
        layouter.constrain_instance(
            token_commit.inner().x().cell(),
            config.primary,
            MINT_TOKCOMX_OFFSET,
        )?;

        layouter.constrain_instance(
            token_commit.inner().y().cell(),
            config.primary,
            MINT_TOKCOMY_OFFSET,
        )?;

        // At this point we've enforced all of our public inputs.
        Ok(())
    }
}
