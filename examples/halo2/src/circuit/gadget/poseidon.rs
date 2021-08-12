//! Gadget and chips for the Poseidon algebraic hash function.

use std::array;
use std::fmt;

use halo2::{
    arithmetic::FieldExt,
    circuit::{Chip, Layouter},
    plonk::Error,
};

mod pow5t3;
pub use pow5t3::{Pow5T3Chip, Pow5T3Config, StateWord};

use crate::primitives::poseidon::{ConstantLength, Domain, Spec, Sponge, SpongeState, State};

/// The set of circuit instructions required to use the Poseidon permutation.
pub trait PoseidonInstructions<F: FieldExt, S: Spec<F, T, RATE>, const T: usize, const RATE: usize>:
    Chip<F>
{
    /// Variable representing the word over which the Poseidon permutation operates.
    type Word: Copy + fmt::Debug;

    /// Applies the Poseidon permutation to the given state.
    fn permute(
        &self,
        layouter: &mut impl Layouter<F>,
        initial_state: &State<Self::Word, T>,
    ) -> Result<State<Self::Word, T>, Error>;
}

/// The set of circuit instructions required to use the [`Duplex`] and [`Hash`] gadgets.
///
/// [`Hash`]: self::Hash
pub trait PoseidonDuplexInstructions<
    F: FieldExt,
    S: Spec<F, T, RATE>,
    const T: usize,
    const RATE: usize,
>: PoseidonInstructions<F, S, T, RATE>
{
    /// Returns the initial empty state for the given domain.
    fn initial_state(
        &self,
        layouter: &mut impl Layouter<F>,
        domain: &impl Domain<F, S, T, RATE>,
    ) -> Result<State<Self::Word, T>, Error>;

    /// Pads the given input (according to the specified domain) and adds it to the state.
    fn pad_and_add(
        &self,
        layouter: &mut impl Layouter<F>,
        domain: &impl Domain<F, S, T, RATE>,
        initial_state: &State<Self::Word, T>,
        input: &SpongeState<Self::Word, RATE>,
    ) -> Result<State<Self::Word, T>, Error>;

    /// Extracts sponge output from the given state.
    fn get_output(state: &State<Self::Word, T>) -> SpongeState<Self::Word, RATE>;
}

/// A word over which the Poseidon permutation operates.
pub struct Word<
    F: FieldExt,
    PoseidonChip: PoseidonInstructions<F, S, T, RATE>,
    S: Spec<F, T, RATE>,
    const T: usize,
    const RATE: usize,
> {
    inner: PoseidonChip::Word,
}

impl<
        F: FieldExt,
        PoseidonChip: PoseidonInstructions<F, S, T, RATE>,
        S: Spec<F, T, RATE>,
        const T: usize,
        const RATE: usize,
    > Word<F, PoseidonChip, S, T, RATE>
{
    pub fn inner(&self) -> PoseidonChip::Word {
        self.inner
    }

    pub fn from_inner(inner: PoseidonChip::Word) -> Self {
        Self { inner }
    }
}

fn poseidon_duplex<
    F: FieldExt,
    PoseidonChip: PoseidonDuplexInstructions<F, S, T, RATE>,
    S: Spec<F, T, RATE>,
    D: Domain<F, S, T, RATE>,
    const T: usize,
    const RATE: usize,
>(
    chip: &PoseidonChip,
    mut layouter: impl Layouter<F>,
    domain: &D,
    state: &mut State<PoseidonChip::Word, T>,
    input: &SpongeState<PoseidonChip::Word, RATE>,
) -> Result<SpongeState<PoseidonChip::Word, RATE>, Error> {
    *state = chip.pad_and_add(&mut layouter, domain, state, input)?;
    *state = chip.permute(&mut layouter, state)?;
    Ok(PoseidonChip::get_output(state))
}

/// A Poseidon duplex sponge.
pub struct Duplex<
    F: FieldExt,
    PoseidonChip: PoseidonDuplexInstructions<F, S, T, RATE>,
    S: Spec<F, T, RATE>,
    D: Domain<F, S, T, RATE>,
    const T: usize,
    const RATE: usize,
> {
    chip: PoseidonChip,
    sponge: Sponge<PoseidonChip::Word, RATE>,
    state: State<PoseidonChip::Word, T>,
    domain: D,
}

impl<
        F: FieldExt,
        PoseidonChip: PoseidonDuplexInstructions<F, S, T, RATE>,
        S: Spec<F, T, RATE>,
        D: Domain<F, S, T, RATE>,
        const T: usize,
        const RATE: usize,
    > Duplex<F, PoseidonChip, S, D, T, RATE>
{
    /// Constructs a new duplex sponge for the given Poseidon specification.
    pub fn new(
        chip: PoseidonChip,
        mut layouter: impl Layouter<F>,
        domain: D,
    ) -> Result<Self, Error> {
        chip.initial_state(&mut layouter, &domain)
            .map(|state| Duplex {
                chip,
                sponge: Sponge::Absorbing([None; RATE]),
                state,
                domain,
            })
    }

    /// Absorbs an element into the sponge.
    pub fn absorb(
        &mut self,
        mut layouter: impl Layouter<F>,
        value: Word<F, PoseidonChip, S, T, RATE>,
    ) -> Result<(), Error> {
        match self.sponge {
            Sponge::Absorbing(ref mut input) => {
                for entry in input.iter_mut() {
                    if entry.is_none() {
                        *entry = Some(value.inner);
                        return Ok(());
                    }
                }

                // We've already absorbed as many elements as we can
                let _ = poseidon_duplex(
                    &self.chip,
                    layouter.namespace(|| "PoseidonDuplex"),
                    &self.domain,
                    &mut self.state,
                    input,
                )?;
                self.sponge = Sponge::absorb(value.inner);
            }
            Sponge::Squeezing(_) => {
                // Drop the remaining output elements
                self.sponge = Sponge::absorb(value.inner);
            }
        }

        Ok(())
    }

    /// Squeezes an element from the sponge.
    pub fn squeeze(
        &mut self,
        mut layouter: impl Layouter<F>,
    ) -> Result<Word<F, PoseidonChip, S, T, RATE>, Error> {
        loop {
            match self.sponge {
                Sponge::Absorbing(ref input) => {
                    self.sponge = Sponge::Squeezing(poseidon_duplex(
                        &self.chip,
                        layouter.namespace(|| "PoseidonDuplex"),
                        &self.domain,
                        &mut self.state,
                        input,
                    )?);
                }
                Sponge::Squeezing(ref mut output) => {
                    for entry in output.iter_mut() {
                        if let Some(inner) = entry.take() {
                            return Ok(Word { inner });
                        }
                    }

                    // We've already squeezed out all available elements
                    self.sponge = Sponge::Absorbing([None; RATE]);
                }
            }
        }
    }
}

/// A Poseidon hash function, built around a duplex sponge.
pub struct Hash<
    F: FieldExt,
    PoseidonChip: PoseidonDuplexInstructions<F, S, T, RATE>,
    S: Spec<F, T, RATE>,
    D: Domain<F, S, T, RATE>,
    const T: usize,
    const RATE: usize,
> {
    duplex: Duplex<F, PoseidonChip, S, D, T, RATE>,
}

impl<
        F: FieldExt,
        PoseidonChip: PoseidonDuplexInstructions<F, S, T, RATE>,
        S: Spec<F, T, RATE>,
        D: Domain<F, S, T, RATE>,
        const T: usize,
        const RATE: usize,
    > Hash<F, PoseidonChip, S, D, T, RATE>
{
    /// Initializes a new hasher.
    pub fn init(chip: PoseidonChip, layouter: impl Layouter<F>, domain: D) -> Result<Self, Error> {
        Duplex::new(chip, layouter, domain).map(|duplex| Hash { duplex })
    }
}

impl<
        F: FieldExt,
        PoseidonChip: PoseidonDuplexInstructions<F, S, T, RATE>,
        S: Spec<F, T, RATE>,
        const T: usize,
        const RATE: usize,
        const L: usize,
    > Hash<F, PoseidonChip, S, ConstantLength<L>, T, RATE>
{
    /// Hashes the given input.
    pub fn hash(
        mut self,
        mut layouter: impl Layouter<F>,
        message: [Word<F, PoseidonChip, S, T, RATE>; L],
    ) -> Result<Word<F, PoseidonChip, S, T, RATE>, Error> {
        for (i, value) in array::IntoIter::new(message).enumerate() {
            self.duplex
                .absorb(layouter.namespace(|| format!("absorb_{}", i)), value)?;
        }
        self.duplex.squeeze(layouter.namespace(|| "squeeze"))
    }
}
