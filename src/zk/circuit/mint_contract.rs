use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointBaseField, FixedPointShort, ScalarFixed, ScalarFixedShort,
    },
    poseidon::{
        primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
        Pow5Config as PoseidonConfig,
    },
    sinsemilla::chip::{SinsemillaChip, SinsemillaConfig},
    utilities::lookup_range_check::LookupRangeCheckConfig,
};
use halo2_proofs::{
    circuit::{floor_planner, AssignedCell, Layouter, Value},
    pasta::{pallas, Fp},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};

use crate::{
    crypto::constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV,
    },
    zk::assign_free_advice,
};

#[derive(Clone, Debug)]
pub struct MintConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
    sinsemilla_config:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
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
    /// X coordinate for public key
    pub pub_x: Value<pallas::Base>,
    /// Y coordinate for public key
    pub pub_y: Value<pallas::Base>,
    /// The value of this coin
    pub value: Value<pallas::Base>,
    /// The token ID
    pub token: Value<pallas::Base>,
    /// Unique serial number corresponding to this coin
    pub serial: Value<pallas::Base>,
    /// Random blinding factor for coin
    pub coin_blind: Value<pallas::Base>,
    /// Allows composing this ZK proof to invoke other contracts
    pub spend_hook: Value<pallas::Base>,
    /// Data passed from this coin to the invoked contract
    pub user_data: Value<pallas::Base>,
    /// Random blinding factor for value commitment
    pub value_blind: Value<pallas::Scalar>,
    /// Random blinding factor for the token ID
    pub token_blind: Value<pallas::Scalar>,
}

impl Circuit<pallas::Base> for MintContract {
    type Config = MintConfig;
    type FloorPlanner = floor_planner::V1;

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

        let ecc_lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];

        let poseidon_lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];

        let rc_a = poseidon_lagrange_coeffs[0..3].try_into().unwrap();
        let rc_b = poseidon_lagrange_coeffs[3..6].try_into().unwrap();

        // Also use the first Lagrange coefficient column for loading global constants.
        meta.enable_constant(ecc_lagrange_coeffs[0]);

        // Use one of the right-most advice columns for all of our range checks.
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        // Configuration for curve point operations.
        // This uses 10 advice columns and spans the whole circuit.
        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices,
            ecc_lagrange_coeffs,
            range_check,
        );

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
            meta,
            advices[7..10].try_into().unwrap(),
            advices[6],
            rc_a,
            rc_b,
        );

        let sinsemilla_config = SinsemillaChip::configure(
            meta,
            advices[..5].try_into().unwrap(),
            advices[6],
            ecc_lagrange_coeffs[0],
            lookup,
            range_check,
        );

        MintConfig { primary, advices, ecc_config, poseidon_config, sinsemilla_config }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_config.clone(), &mut layouter)?;

        let ecc_chip = config.ecc_chip();

        let pub_x = assign_free_advice(
            layouter.namespace(|| "load pubkey x"),
            config.advices[6],
            self.pub_x,
        )?;

        let pub_y = assign_free_advice(
            layouter.namespace(|| "load pubkey y"),
            config.advices[6],
            self.pub_y,
        )?;

        let value =
            assign_free_advice(layouter.namespace(|| "load value"), config.advices[6], self.value)?;

        let token =
            assign_free_advice(layouter.namespace(|| "load token"), config.advices[6], self.token)?;

        let serial = assign_free_advice(
            layouter.namespace(|| "load serial"),
            config.advices[6],
            self.serial,
        )?;

        let spend_hook = assign_free_advice(
            layouter.namespace(|| "load spend_hook"),
            config.advices[6],
            self.spend_hook,
        )?;

        let user_data = assign_free_advice(
            layouter.namespace(|| "load user_data"),
            config.advices[6],
            self.user_data,
        )?;

        let coin_blind = assign_free_advice(
            layouter.namespace(|| "load coin_blind"),
            config.advices[6],
            self.coin_blind,
        )?;

        // =========
        // Coin hash
        // =========
        let coin = {
            let poseidon_message = [
                pub_x,
                pub_y,
                value.clone(),
                token.clone(),
                serial,
                spend_hook,
                user_data,
                coin_blind,
            ];

            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<8>,
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
        let one = assign_free_advice(
            layouter.namespace(|| "load constant one"),
            config.advices[6],
            Value::known(pallas::Base::one()),
        )?;

        // v * G_1
        let (commitment, _) = {
            let value_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), ValueCommitV);
            let value = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "value"),
                (value, one),
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
            let value_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
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
        let commitment = {
            let token_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            token_commit_v.mul(layouter.namespace(|| "[token] NullifierK"), token)?
        };

        // r_A * G_2
        let (blind, _rca) = {
            let rca = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "token_blind"),
                self.token_blind,
            )?;
            let token_commit_r =
                FixedPoint::from_inner(ecc_chip, OrchardFixedBasesFull::ValueCommitR);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{
            keypair::PublicKey,
            proof::{ProvingKey, VerifyingKey},
            util::{pedersen_commitment_base, pedersen_commitment_u64},
            Proof,
        },
        Result,
    };
    use halo2_gadgets::poseidon::{
        primitives as poseidon,
        primitives::{ConstantLength, P128Pow5T3},
    };
    use halo2_proofs::{
        circuit::Value,
        dev::{CircuitLayout, MockProver},
    };
    use pasta_curves::{
        arithmetic::CurveAffine,
        group::{ff::Field, Curve},
    };
    use rand::rngs::OsRng;
    use std::time::Instant;

    #[test]
    fn mint_circuit_assert() -> Result<()> {
        let value = 42;
        let token_id = pallas::Base::random(&mut OsRng);
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Scalar::random(&mut OsRng);
        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);
        let public_key = PublicKey::random(&mut OsRng);
        let coords = public_key.0.to_affine().coordinates().unwrap();
        let spend_hook = pallas::Base::random(&mut OsRng);
        let user_data = pallas::Base::random(&mut OsRng);

        let msg = [
            *coords.x(),
            *coords.y(),
            pallas::Base::from(value),
            token_id,
            serial,
            spend_hook,
            user_data,
            coin_blind,
        ];
        let coin = poseidon::Hash::<_, P128Pow5T3, ConstantLength<8>, 3, 2>::init().hash(msg);

        let value_commit = pedersen_commitment_u64(value, value_blind);
        let value_coords = value_commit.to_affine().coordinates().unwrap();

        let token_commit = pedersen_commitment_base(token_id, token_blind);
        let token_coords = token_commit.to_affine().coordinates().unwrap();

        let public_inputs =
            vec![coin, *value_coords.x(), *value_coords.y(), *token_coords.x(), *token_coords.y()];

        let circuit = MintContract {
            pub_x: Value::known(*coords.x()),
            pub_y: Value::known(*coords.y()),
            value: Value::known(pallas::Base::from(value)),
            token: Value::known(token_id),
            serial: Value::known(serial),
            coin_blind: Value::known(coin_blind),
            spend_hook: Value::known(spend_hook),
            user_data: Value::known(user_data),
            value_blind: Value::known(value_blind),
            token_blind: Value::known(token_blind),
        };

        use plotters::prelude::*;
        let root =
            BitMapBackend::new("target/mint_circuit_layout.png", (3840, 2160)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.titled("Mint Circuit Layout", ("sans-serif", 60)).unwrap();
        CircuitLayout::default().render(11, &circuit, &root).unwrap();

        let prover = MockProver::run(11, &circuit, vec![public_inputs.clone()])?;
        prover.assert_satisfied();

        let now = Instant::now();
        let proving_key = ProvingKey::build(11, &circuit);
        println!("ProvingKey built [{:?}]", now.elapsed());
        let now = Instant::now();
        let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;
        println!("Proof created [{:?}]", now.elapsed());

        let circuit = MintContract::default();
        let now = Instant::now();
        let verifying_key = VerifyingKey::build(11, &circuit);
        println!("VerifyingKey built [{:?}]", now.elapsed());
        let now = Instant::now();
        proof.verify(&verifying_key, &public_inputs)?;
        println!("Proof verified [{:?}]", now.elapsed());

        println!("Proof size [{} kB]", proof.as_ref().len() as f64 / 1024.0);

        Ok(())
    }
}
