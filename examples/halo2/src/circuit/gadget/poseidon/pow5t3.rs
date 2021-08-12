use std::iter;

use halo2::{
    arithmetic::FieldExt,
    circuit::{Cell, Chip, Layouter, Region},
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Fixed, Selector},
    poly::Rotation,
};

use super::{PoseidonDuplexInstructions, PoseidonInstructions};
use crate::circuit::gadget::utilities::{CellValue, Var};
use crate::primitives::poseidon::{Domain, Mds, Spec, SpongeState, State};

const WIDTH: usize = 3;

/// Configuration for an [`Pow5T3Chip`].
#[derive(Clone, Debug)]
pub struct Pow5T3Config<F: FieldExt> {
    //pub(in crate::circuit) state: [Column<Advice>; WIDTH],
    pub state: [Column<Advice>; WIDTH],
    partial_sbox: Column<Advice>,
    rc_a: [Column<Fixed>; WIDTH],
    rc_b: [Column<Fixed>; WIDTH],
    s_full: Selector,
    s_partial: Selector,
    s_pad_and_add: Selector,

    half_full_rounds: usize,
    half_partial_rounds: usize,
    alpha: [u64; 4],
    round_constants: Vec<[F; WIDTH]>,
    m_reg: Mds<F, WIDTH>,
    m_inv: Mds<F, WIDTH>,
}

/// A Poseidon chip using an $x^5$ S-Box, with a width of 3, suitable for a 2:1 reduction.
#[derive(Debug)]
pub struct Pow5T3Chip<F: FieldExt> {
    config: Pow5T3Config<F>,
}

impl<F: FieldExt> Pow5T3Chip<F> {
    /// Configures this chip for use in a circuit.
    ///
    /// # Side-effects
    ///
    /// All columns in `state` will be equality-enabled.
    //
    // TODO: Does the rate need to be hard-coded here, or only the width? It probably
    // needs to be known wherever we implement the hashing gadget, but it isn't strictly
    // necessary for the permutation.
    pub fn configure<S: Spec<F, WIDTH, 2>>(
        meta: &mut ConstraintSystem<F>,
        spec: S,
        state: [Column<Advice>; WIDTH],
        partial_sbox: Column<Advice>,
        rc_a: [Column<Fixed>; WIDTH],
        rc_b: [Column<Fixed>; WIDTH],
    ) -> Pow5T3Config<F> {
        // Generate constants for the Poseidon permutation.
        // This gadget requires R_F and R_P to be even.
        assert!(S::full_rounds() & 1 == 0);
        assert!(S::partial_rounds() & 1 == 0);
        let half_full_rounds = S::full_rounds() / 2;
        let half_partial_rounds = S::partial_rounds() / 2;
        let (round_constants, m_reg, m_inv) = spec.constants();

        // This allows state words to be initialized (by constraining them equal to fixed
        // values), and used in a permutation from an arbitrary region. rc_a is used in
        // every permutation round, while rc_b is empty in the initial and final full
        // rounds, so we use rc_b as "scratch space" for fixed values (enabling potential
        // layouter optimisations).
        for column in iter::empty()
            .chain(state.iter().cloned().map(|c| c.into()))
            .chain(rc_b.iter().cloned().map(|c| c.into()))
        {
            meta.enable_equality(column);
        }

        let s_full = meta.selector();
        let s_partial = meta.selector();
        let s_pad_and_add = meta.selector();

        let alpha = [5, 0, 0, 0];
        let pow_5 = |v: Expression<F>| {
            let v2 = v.clone() * v.clone();
            v2.clone() * v2 * v
        };

        meta.create_gate("full round", |meta| {
            let cur_0 = meta.query_advice(state[0], Rotation::cur());
            let cur_1 = meta.query_advice(state[1], Rotation::cur());
            let cur_2 = meta.query_advice(state[2], Rotation::cur());
            let next = [
                meta.query_advice(state[0], Rotation::next()),
                meta.query_advice(state[1], Rotation::next()),
                meta.query_advice(state[2], Rotation::next()),
            ];

            let rc_0 = meta.query_fixed(rc_a[0], Rotation::cur());
            let rc_1 = meta.query_fixed(rc_a[1], Rotation::cur());
            let rc_2 = meta.query_fixed(rc_a[2], Rotation::cur());

            let s_full = meta.query_selector(s_full);

            let full_round = |next_idx: usize| {
                s_full.clone()
                    * (pow_5(cur_0.clone() + rc_0.clone()) * m_reg[next_idx][0]
                        + pow_5(cur_1.clone() + rc_1.clone()) * m_reg[next_idx][1]
                        + pow_5(cur_2.clone() + rc_2.clone()) * m_reg[next_idx][2]
                        - next[next_idx].clone())
            };

            vec![full_round(0), full_round(1), full_round(2)]
        });

        meta.create_gate("partial round", |meta| {
            let cur_0 = meta.query_advice(state[0], Rotation::cur());
            let cur_1 = meta.query_advice(state[1], Rotation::cur());
            let cur_2 = meta.query_advice(state[2], Rotation::cur());
            let mid_0 = meta.query_advice(partial_sbox, Rotation::cur());
            let next_0 = meta.query_advice(state[0], Rotation::next());
            let next_1 = meta.query_advice(state[1], Rotation::next());
            let next_2 = meta.query_advice(state[2], Rotation::next());

            let rc_a0 = meta.query_fixed(rc_a[0], Rotation::cur());
            let rc_a1 = meta.query_fixed(rc_a[1], Rotation::cur());
            let rc_a2 = meta.query_fixed(rc_a[2], Rotation::cur());
            let rc_b0 = meta.query_fixed(rc_b[0], Rotation::cur());
            let rc_b1 = meta.query_fixed(rc_b[1], Rotation::cur());
            let rc_b2 = meta.query_fixed(rc_b[2], Rotation::cur());

            let s_partial = meta.query_selector(s_partial);

            let partial_round_linear = |idx: usize, rc_b: Expression<F>| {
                s_partial.clone()
                    * (mid_0.clone() * m_reg[idx][0]
                        + (cur_1.clone() + rc_a1.clone()) * m_reg[idx][1]
                        + (cur_2.clone() + rc_a2.clone()) * m_reg[idx][2]
                        + rc_b
                        - (next_0.clone() * m_inv[idx][0]
                            + next_1.clone() * m_inv[idx][1]
                            + next_2.clone() * m_inv[idx][2]))
            };

            vec![
                s_partial.clone() * (pow_5(cur_0 + rc_a0) - mid_0.clone()),
                s_partial.clone()
                    * (pow_5(
                        mid_0.clone() * m_reg[0][0]
                            + (cur_1.clone() + rc_a1.clone()) * m_reg[0][1]
                            + (cur_2.clone() + rc_a2.clone()) * m_reg[0][2]
                            + rc_b0,
                    ) - (next_0.clone() * m_inv[0][0]
                        + next_1.clone() * m_inv[0][1]
                        + next_2.clone() * m_inv[0][2])),
                partial_round_linear(1, rc_b1),
                partial_round_linear(2, rc_b2),
            ]
        });

        meta.create_gate("pad-and-add", |meta| {
            let initial_state_0 = meta.query_advice(state[0], Rotation::prev());
            let initial_state_1 = meta.query_advice(state[1], Rotation::prev());
            let initial_state_2 = meta.query_advice(state[2], Rotation::prev());
            let input_0 = meta.query_advice(state[0], Rotation::cur());
            let input_1 = meta.query_advice(state[1], Rotation::cur());
            let output_state_0 = meta.query_advice(state[0], Rotation::next());
            let output_state_1 = meta.query_advice(state[1], Rotation::next());
            let output_state_2 = meta.query_advice(state[2], Rotation::next());

            let s_pad_and_add = meta.query_selector(s_pad_and_add);

            let pad_and_add = |initial_state, input, output_state| {
                // We pad the input by storing the required padding in fixed columns and
                // then constraining the corresponding input columns to be equal to it.
                s_pad_and_add.clone() * (initial_state + input - output_state)
            };

            vec![
                pad_and_add(initial_state_0, input_0, output_state_0),
                pad_and_add(initial_state_1, input_1, output_state_1),
                // The capacity element is never altered by the input.
                s_pad_and_add * (initial_state_2 - output_state_2),
            ]
        });

        Pow5T3Config {
            state,
            partial_sbox,
            rc_a,
            rc_b,
            s_full,
            s_partial,
            s_pad_and_add,
            half_full_rounds,
            half_partial_rounds,
            alpha,
            round_constants,
            m_reg,
            m_inv,
        }
    }

    pub fn construct(config: Pow5T3Config<F>) -> Self {
        Pow5T3Chip { config }
    }
}

impl<F: FieldExt> Chip<F> for Pow5T3Chip<F> {
    type Config = Pow5T3Config<F>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: FieldExt, S: Spec<F, WIDTH, 2>> PoseidonInstructions<F, S, WIDTH, 2> for Pow5T3Chip<F> {
    type Word = StateWord<F>;

    fn permute(
        &self,
        layouter: &mut impl Layouter<F>,
        initial_state: &State<Self::Word, WIDTH>,
    ) -> Result<State<Self::Word, WIDTH>, Error> {
        let config = self.config();

        layouter.assign_region(
            || "permute state",
            |mut region| {
                // Load the initial state into this region.
                let state = Pow5T3State::load(&mut region, config, initial_state)?;

                let state = (0..config.half_full_rounds).fold(Ok(state), |res, r| {
                    res.and_then(|state| state.full_round(&mut region, config, r, r))
                })?;

                let state = (0..config.half_partial_rounds).fold(Ok(state), |res, r| {
                    res.and_then(|state| {
                        state.partial_round(
                            &mut region,
                            config,
                            config.half_full_rounds + 2 * r,
                            config.half_full_rounds + r,
                        )
                    })
                })?;

                let state = (0..config.half_full_rounds).fold(Ok(state), |res, r| {
                    res.and_then(|state| {
                        state.full_round(
                            &mut region,
                            config,
                            config.half_full_rounds + 2 * config.half_partial_rounds + r,
                            config.half_full_rounds + config.half_partial_rounds + r,
                        )
                    })
                })?;

                Ok(state.0)
            },
        )
    }
}

impl<F: FieldExt, S: Spec<F, WIDTH, 2>> PoseidonDuplexInstructions<F, S, WIDTH, 2>
    for Pow5T3Chip<F>
{
    fn initial_state(
        &self,
        layouter: &mut impl Layouter<F>,
        domain: &impl Domain<F, S, WIDTH, 2>,
    ) -> Result<State<Self::Word, WIDTH>, Error> {
        let config = self.config();
        layouter.assign_region(
            || format!("initial state for domain {:?}", domain),
            |mut region| {
                let mut load_state_word = |i: usize, value: F| {
                    let var = region.assign_advice_from_constant(
                        || format!("state_{}", i),
                        config.state[i],
                        0,
                        value,
                    )?;
                    Ok(StateWord {
                        var,
                        value: Some(value),
                    })
                };

                Ok([
                    load_state_word(0, F::zero())?,
                    load_state_word(1, F::zero())?,
                    load_state_word(2, domain.initial_capacity_element())?,
                ])
            },
        )
    }

    fn pad_and_add(
        &self,
        layouter: &mut impl Layouter<F>,
        domain: &impl Domain<F, S, WIDTH, 2>,
        initial_state: &State<Self::Word, WIDTH>,
        input: &SpongeState<Self::Word, 2>,
    ) -> Result<State<Self::Word, WIDTH>, Error> {
        let config = self.config();
        layouter.assign_region(
            || format!("pad-and-add for domain {:?}", domain),
            |mut region| {
                config.s_pad_and_add.enable(&mut region, 1)?;

                // Load the initial state into this region.
                let mut load_state_word = |i: usize| {
                    let value = initial_state[i].value;
                    let var = region.assign_advice(
                        || format!("load state_{}", i),
                        config.state[i],
                        0,
                        || value.ok_or(Error::SynthesisError),
                    )?;
                    region.constrain_equal(initial_state[i].var, var)?;
                    Ok(StateWord { var, value })
                };
                let initial_state = [
                    load_state_word(0)?,
                    load_state_word(1)?,
                    load_state_word(2)?,
                ];

                let padding_values = domain.padding();

                // Load the input and padding into this region.
                let mut load_input_word = |i: usize| {
                    let (constraint_var, value) = match (input[i], padding_values[i]) {
                        (Some(word), None) => (word.var, word.value),
                        (None, Some(padding_value)) => {
                            let padding_var = region.assign_fixed(
                                || format!("load pad_{}", i),
                                config.rc_b[i],
                                1,
                                || Ok(padding_value),
                            )?;
                            (padding_var, Some(padding_value))
                        }
                        _ => panic!("Input and padding don't match"),
                    };
                    let var = region.assign_advice(
                        || format!("load input_{}", i),
                        config.state[i],
                        1,
                        || value.ok_or(Error::SynthesisError),
                    )?;
                    region.constrain_equal(constraint_var, var)?;

                    Ok(StateWord { var, value })
                };
                let input = [load_input_word(0)?, load_input_word(1)?];

                // Constrain the output.
                let mut constrain_output_word = |i: usize| {
                    let value = initial_state[i].value.and_then(|initial_word| {
                        input
                            .get(i)
                            .map(|word| word.value)
                            // The capacity element is never altered by the input.
                            .unwrap_or_else(|| Some(F::zero()))
                            .map(|input_word| initial_word + input_word)
                    });
                    let var = region.assign_advice(
                        || format!("load output_{}", i),
                        config.state[i],
                        2,
                        || value.ok_or(Error::SynthesisError),
                    )?;
                    Ok(StateWord { var, value })
                };

                Ok([
                    constrain_output_word(0)?,
                    constrain_output_word(1)?,
                    constrain_output_word(2)?,
                ])
            },
        )
    }

    fn get_output(state: &State<Self::Word, WIDTH>) -> SpongeState<Self::Word, 2> {
        [Some(state[0]), Some(state[1])]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StateWord<F: FieldExt> {
    var: Cell,
    value: Option<F>,
}

impl<F: FieldExt> StateWord<F> {
    pub fn new(var: Cell, value: Option<F>) -> Self {
        Self { var, value }
    }
}

impl<F: FieldExt> From<StateWord<F>> for CellValue<F> {
    fn from(state_word: StateWord<F>) -> CellValue<F> {
        CellValue::new(state_word.var, state_word.value)
    }
}

#[derive(Debug)]
struct Pow5T3State<F: FieldExt>([StateWord<F>; WIDTH]);

impl<F: FieldExt> Pow5T3State<F> {
    fn full_round(
        self,
        region: &mut Region<F>,
        config: &Pow5T3Config<F>,
        round: usize,
        offset: usize,
    ) -> Result<Self, Error> {
        Self::round(region, config, round, offset, config.s_full, |_| {
            let q_0 = self.0[0]
                .value
                .map(|v| v + config.round_constants[round][0]);
            let q_1 = self.0[1]
                .value
                .map(|v| v + config.round_constants[round][1]);
            let q_2 = self.0[2]
                .value
                .map(|v| v + config.round_constants[round][2]);

            let r_0 = q_0.map(|v| v.pow(&config.alpha));
            let r_1 = q_1.map(|v| v.pow(&config.alpha));
            let r_2 = q_2.map(|v| v.pow(&config.alpha));

            let m = &config.m_reg;
            let r = r_0.and_then(|r_0| r_1.and_then(|r_1| r_2.map(|r_2| [r_0, r_1, r_2])));

            Ok((
                round + 1,
                [
                    r.map(|r| m[0][0] * r[0] + m[0][1] * r[1] + m[0][2] * r[2]),
                    r.map(|r| m[1][0] * r[0] + m[1][1] * r[1] + m[1][2] * r[2]),
                    r.map(|r| m[2][0] * r[0] + m[2][1] * r[1] + m[2][2] * r[2]),
                ],
            ))
        })
    }

    fn partial_round(
        self,
        region: &mut Region<F>,
        config: &Pow5T3Config<F>,
        round: usize,
        offset: usize,
    ) -> Result<Self, Error> {
        Self::round(region, config, round, offset, config.s_partial, |region| {
            let m = &config.m_reg;

            let p = self.0[0].value.and_then(|p_0| {
                self.0[1]
                    .value
                    .and_then(|p_1| self.0[2].value.map(|p_2| [p_0, p_1, p_2]))
            });

            let r = p.map(|p| {
                [
                    (p[0] + config.round_constants[round][0]).pow(&config.alpha),
                    p[1] + config.round_constants[round][1],
                    p[2] + config.round_constants[round][2],
                ]
            });

            region.assign_advice(
                || format!("round_{} partial_sbox", round),
                config.partial_sbox,
                offset,
                || r.map(|r| r[0]).ok_or(Error::SynthesisError),
            )?;

            let p_mid = r.map(|r| {
                [
                    m[0][0] * r[0] + m[0][1] * r[1] + m[0][2] * r[2],
                    m[1][0] * r[0] + m[1][1] * r[1] + m[1][2] * r[2],
                    m[2][0] * r[0] + m[2][1] * r[1] + m[2][2] * r[2],
                ]
            });

            // Load the second round constants.
            let mut load_round_constant = |i: usize| {
                region.assign_fixed(
                    || format!("round_{} rc_{}", round + 1, i),
                    config.rc_b[i],
                    offset,
                    || Ok(config.round_constants[round + 1][i]),
                )
            };
            for i in 0..WIDTH {
                load_round_constant(i)?;
            }

            let r_mid = p_mid.map(|p| {
                [
                    (p[0] + config.round_constants[round + 1][0]).pow(&config.alpha),
                    p[1] + config.round_constants[round + 1][1],
                    p[2] + config.round_constants[round + 1][2],
                ]
            });

            Ok((
                round + 2,
                [
                    r_mid.map(|r| m[0][0] * r[0] + m[0][1] * r[1] + m[0][2] * r[2]),
                    r_mid.map(|r| m[1][0] * r[0] + m[1][1] * r[1] + m[1][2] * r[2]),
                    r_mid.map(|r| m[2][0] * r[0] + m[2][1] * r[1] + m[2][2] * r[2]),
                ],
            ))
        })
    }

    fn load(
        region: &mut Region<F>,
        config: &Pow5T3Config<F>,
        initial_state: &State<StateWord<F>, WIDTH>,
    ) -> Result<Self, Error> {
        let mut load_state_word = |i: usize| {
            let value = initial_state[i].value;
            let var = region.assign_advice(
                || format!("load state_{}", i),
                config.state[i],
                0,
                || value.ok_or(Error::SynthesisError),
            )?;
            region.constrain_equal(initial_state[i].var, var)?;
            Ok(StateWord { var, value })
        };

        Ok(Pow5T3State([
            load_state_word(0)?,
            load_state_word(1)?,
            load_state_word(2)?,
        ]))
    }

    fn round(
        region: &mut Region<F>,
        config: &Pow5T3Config<F>,
        round: usize,
        offset: usize,
        round_gate: Selector,
        round_fn: impl FnOnce(&mut Region<F>) -> Result<(usize, [Option<F>; WIDTH]), Error>,
    ) -> Result<Self, Error> {
        // Enable the required gate.
        round_gate.enable(region, offset)?;

        // Load the round constants.
        let mut load_round_constant = |i: usize| {
            region.assign_fixed(
                || format!("round_{} rc_{}", round, i),
                config.rc_a[i],
                offset,
                || Ok(config.round_constants[round][i]),
            )
        };
        for i in 0..WIDTH {
            load_round_constant(i)?;
        }

        // Compute the next round's state.
        let (next_round, next_state) = round_fn(region)?;

        let mut next_state_word = |i: usize| {
            let value = next_state[i];
            let var = region.assign_advice(
                || format!("round_{} state_{}", next_round, i),
                config.state[i],
                offset + 1,
                || value.ok_or(Error::SynthesisError),
            )?;
            Ok(StateWord { var, value })
        };

        Ok(Pow5T3State([
            next_state_word(0)?,
            next_state_word(1)?,
            next_state_word(2)?,
        ]))
    }
}

#[cfg(test)]
mod tests {
    use ff::PrimeField;
    use halo2::{
        arithmetic::FieldExt,
        circuit::{Layouter, SimpleFloorPlanner},
        dev::MockProver,
        pasta::Fp,
        plonk::{Circuit, ConstraintSystem, Error},
    };
    use pasta_curves::pallas;

    use super::{PoseidonInstructions, Pow5T3Chip, Pow5T3Config, StateWord, WIDTH};
    use crate::{
        circuit::gadget::poseidon::{Hash, Word},
        primitives::poseidon::{self, ConstantLength, OrchardNullifier, Spec},
    };

    struct PermuteCircuit {}

    impl Circuit<Fp> for PermuteCircuit {
        type Config = Pow5T3Config<Fp>;
        type FloorPlanner = SimpleFloorPlanner;

        fn without_witnesses(&self) -> Self {
            PermuteCircuit {}
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Pow5T3Config<Fp> {
            let state = [
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
            ];
            let partial_sbox = meta.advice_column();

            let rc_a = [
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
            ];
            let rc_b = [
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
            ];

            Pow5T3Chip::configure(meta, OrchardNullifier, state, partial_sbox, rc_a, rc_b)
        }

        fn synthesize(
            &self,
            config: Pow5T3Config<Fp>,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), Error> {
            let initial_state = layouter.assign_region(
                || "prepare initial state",
                |mut region| {
                    let mut state_word = |i: usize| {
                        let value = Some(Fp::from(i as u64));
                        let var = region.assign_advice(
                            || format!("load state_{}", i),
                            config.state[i],
                            0,
                            || value.ok_or(Error::SynthesisError),
                        )?;
                        Ok(StateWord { var, value })
                    };

                    Ok([state_word(0)?, state_word(1)?, state_word(2)?])
                },
            )?;

            let chip = Pow5T3Chip::construct(config.clone());
            let final_state = <Pow5T3Chip<_> as PoseidonInstructions<
                Fp,
                OrchardNullifier,
                WIDTH,
                2,
            >>::permute(&chip, &mut layouter, &initial_state)?;

            // For the purpose of this test, compute the real final state inline.
            let mut expected_final_state = [Fp::zero(), Fp::one(), Fp::from_u64(2)];
            let (round_constants, mds, _) = OrchardNullifier.constants();
            poseidon::permute::<_, OrchardNullifier, WIDTH, 2>(
                &mut expected_final_state,
                &mds,
                &round_constants,
            );

            layouter.assign_region(
                || "constrain final state",
                |mut region| {
                    let mut final_state_word = |i: usize| {
                        let var = region.assign_advice(
                            || format!("load final_state_{}", i),
                            config.state[i],
                            0,
                            || Ok(expected_final_state[i]),
                        )?;
                        region.constrain_equal(final_state[i].var, var)
                    };

                    final_state_word(0)?;
                    final_state_word(1)?;
                    final_state_word(2)
                },
            )
        }
    }

    #[test]
    fn poseidon_permute() {
        let k = 6;
        let circuit = PermuteCircuit {};
        let prover = MockProver::run(k, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()))
    }

    #[derive(Default)]
    struct HashCircuit {
        message: Option<[Fp; 2]>,
        // For the purpose of this test, witness the result.
        // TODO: Move this into an instance column.
        output: Option<Fp>,
    }

    impl Circuit<Fp> for HashCircuit {
        type Config = Pow5T3Config<Fp>;
        type FloorPlanner = SimpleFloorPlanner;

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Pow5T3Config<Fp> {
            let state = [
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
            ];
            let partial_sbox = meta.advice_column();

            let rc_a = [
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
            ];
            let rc_b = [
                meta.fixed_column(),
                meta.fixed_column(),
                meta.fixed_column(),
            ];

            meta.enable_constant(rc_b[0]);

            Pow5T3Chip::configure(meta, OrchardNullifier, state, partial_sbox, rc_a, rc_b)
        }

        fn synthesize(
            &self,
            config: Pow5T3Config<Fp>,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), Error> {
            let chip = Pow5T3Chip::construct(config.clone());

            let message = layouter.assign_region(
                || "load message",
                |mut region| {
                    let mut message_word = |i: usize| {
                        let value = self.message.map(|message_vals| message_vals[i]);
                        let var = region.assign_advice(
                            || format!("load message_{}", i),
                            config.state[i],
                            0,
                            || value.ok_or(Error::SynthesisError),
                        )?;
                        Ok(Word::<_, _, OrchardNullifier, WIDTH, 2> {
                            inner: StateWord { var, value },
                        })
                    };

                    Ok([message_word(0)?, message_word(1)?])
                },
            )?;

            let hasher = Hash::init(chip, layouter.namespace(|| "init"), ConstantLength::<2>)?;
            let output = hasher.hash(layouter.namespace(|| "hash"), message)?;

            layouter.assign_region(
                || "constrain output",
                |mut region| {
                    let expected_var = region.assign_advice(
                        || "load output",
                        config.state[0],
                        0,
                        || self.output.ok_or(Error::SynthesisError),
                    )?;
                    let word: StateWord<_> = output.inner;
                    region.constrain_equal(word.var, expected_var)
                },
            )
        }
    }

    #[test]
    fn poseidon_hash() {
        let message = [Fp::rand(), Fp::rand()];
        let output = poseidon::Hash::init(OrchardNullifier, ConstantLength::<2>).hash(message);

        let k = 6;
        let circuit = HashCircuit {
            message: Some(message),
            output: Some(output),
        };
        let prover = MockProver::run(k, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()))
    }

    #[test]
    fn hash_test_vectors() {
        for tv in crate::primitives::poseidon::test_vectors::hash() {
            let message = [
                pallas::Base::from_repr(tv.input[0]).unwrap(),
                pallas::Base::from_repr(tv.input[1]).unwrap(),
            ];
            let output = poseidon::Hash::init(OrchardNullifier, ConstantLength).hash(message);

            let k = 6;
            let circuit = HashCircuit {
                message: Some(message),
                output: Some(output),
            };
            let prover = MockProver::run(k, &circuit, vec![]).unwrap();
            assert_eq!(prover.verify(), Ok(()));
        }
    }

    #[cfg(feature = "dev-graph")]
    #[test]
    fn print_poseidon_chip() {
        use plotters::prelude::*;

        let root = BitMapBackend::new("poseidon-chip-layout.png", (1024, 768)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root
            .titled("Poseidon Chip Layout", ("sans-serif", 60))
            .unwrap();

        let circuit = HashCircuit {
            message: None,
            output: None,
        };
        halo2::dev::CircuitLayout::default()
            .render(6, &circuit, &root)
            .unwrap();
    }
}
