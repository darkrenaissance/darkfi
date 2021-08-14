//! Gadgets for elliptic curve operations.

use std::fmt::Debug;

use halo2::{
    arithmetic::CurveAffine,
    circuit::{Chip, Layouter},
    plonk::Error,
};

use crate::circuit::gadget::utilities::UtilitiesInstructions;

pub mod chip;

/// The set of circuit instructions required to use the ECC gadgets.
pub trait EccInstructions<C: CurveAffine>: Chip<C::Base> + UtilitiesInstructions<C::Base> {
    /// Variable representing an element of the elliptic curve's base field, that
    /// is used as a scalar in variable-base scalar mul.
    ///
    /// It is not true in general that a scalar field element fits in a curve's
    /// base field, and in particular it is untrue for the Pallas curve, whose
    /// scalar field `Fq` is larger than its base field `Fp`.
    ///
    /// However, the only use of variable-base scalar mul in the Orchard protocol
    /// is in deriving diversified addresses `[ivk] g_d`,  and `ivk` is guaranteed
    /// to be in the base field of the curve. (See non-normative notes in
    /// https://zips.z.cash/protocol/nu5.pdf#orchardkeycomponents.)
    type ScalarVar: Clone + Debug;
    /// Variable representing a full-width element of the elliptic curve's
    /// scalar field, to be used for fixed-base scalar mul.
    type ScalarFixed: Clone + Debug;
    /// Variable representing a signed short element of the elliptic curve's
    /// scalar field, to be used for fixed-base scalar mul.
    ///
    /// A `ScalarFixedShort` must be in the range [-(2^64 - 1), 2^64 - 1].
    type ScalarFixedShort: Clone + Debug;
    /// Variable representing an elliptic curve point.
    type Point: Clone + Debug;
    /// Variable representing the affine short Weierstrass x-coordinate of an
    /// elliptic curve point.
    type X: Clone + Debug;
    /// Enumeration of the set of fixed bases to be used in scalar mul with a full-width scalar.
    type FixedPoints: Clone + Debug;
    /// Enumeration of the set of fixed bases to be used in scalar mul with a base field element.
    type FixedPointsBaseField: Clone + Debug;
    /// Enumeration of the set of fixed bases to be used in short signed scalar mul.
    type FixedPointsShort: Clone + Debug;

    /// Constrains point `a` to be equal in value to point `b`.
    fn constrain_equal(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        a: &Self::Point,
        b: &Self::Point,
    ) -> Result<(), Error>;

    /// Witnesses the given point as a private input to the circuit.
    /// This maps the identity to (0, 0) in affine coordinates.
    fn witness_point(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        value: Option<C>,
    ) -> Result<Self::Point, Error>;

    /// Extracts the x-coordinate of a point.
    fn extract_p(point: &Self::Point) -> &Self::X;

    /// Performs incomplete point addition, returning `a + b`.
    ///
    /// This returns an error in exceptional cases.
    fn add_incomplete(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        a: &Self::Point,
        b: &Self::Point,
    ) -> Result<Self::Point, Error>;

    /// Performs complete point addition, returning `a + b`.
    fn add(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        a: &Self::Point,
        b: &Self::Point,
    ) -> Result<Self::Point, Error>;

    /// Performs variable-base scalar multiplication, returning `[scalar] base`.
    /// Multiplication of the identity `[scalar] 𝒪 ` returns an error.
    fn mul(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        scalar: &Self::Var,
        base: &Self::Point,
    ) -> Result<(Self::Point, Self::ScalarVar), Error>;

    /// Performs fixed-base scalar multiplication using a full-width scalar, returning `[scalar] base`.
    fn mul_fixed(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        scalar: Option<C::Scalar>,
        base: &Self::FixedPoints,
    ) -> Result<(Self::Point, Self::ScalarFixed), Error>;

    /// Performs fixed-base scalar multiplication using a short signed scalar, returning
    /// `[magnitude * sign] base`.
    fn mul_fixed_short(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        magnitude_sign: (Self::Var, Self::Var),
        base: &Self::FixedPointsShort,
    ) -> Result<(Self::Point, Self::ScalarFixedShort), Error>;

    /// Performs fixed-base scalar multiplication using a base field element as the scalar.
    /// In the current implementation, this base field element must be output from another
    /// instruction.
    fn mul_fixed_base_field_elem(
        &self,
        layouter: &mut impl Layouter<C::Base>,
        base_field_elem: Self::Var,
        base: &Self::FixedPointsBaseField,
    ) -> Result<Self::Point, Error>;
}

/// An element of the given elliptic curve's base field, that is used as a scalar
/// in variable-base scalar mul.
///
/// It is not true in general that a scalar field element fits in a curve's
/// base field, and in particular it is untrue for the Pallas curve, whose
/// scalar field `Fq` is larger than its base field `Fp`.
///
/// However, the only use of variable-base scalar mul in the Orchard protocol
/// is in deriving diversified addresses `[ivk] g_d`,  and `ivk` is guaranteed
/// to be in the base field of the curve. (See non-normative notes in
/// https://zips.z.cash/protocol/nu5.pdf#orchardkeycomponents.)
#[derive(Debug)]
pub struct ScalarVar<C: CurveAffine, EccChip: EccInstructions<C> + Clone + Debug + Eq> {
    chip: EccChip,
    inner: EccChip::ScalarVar,
}

/// A full-width element of the given elliptic curve's scalar field, to be used for fixed-base scalar mul.
#[derive(Debug)]
pub struct ScalarFixed<C: CurveAffine, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    chip: EccChip,
    inner: EccChip::ScalarFixed,
}

/// A signed short element of the given elliptic curve's scalar field, to be used for fixed-base scalar mul.
#[derive(Debug)]
pub struct ScalarFixedShort<C: CurveAffine, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    chip: EccChip,
    inner: EccChip::ScalarFixedShort,
}

/// An elliptic curve point over the given curve.
#[derive(Copy, Clone, Debug)]
pub struct Point<C: CurveAffine, EccChip: EccInstructions<C> + Clone + Debug + Eq> {
    chip: EccChip,
    inner: EccChip::Point,
}

impl<C: CurveAffine, EccChip: EccInstructions<C> + Clone + Debug + Eq> Point<C, EccChip> {
    /// Constructs a new point with the given value.
    pub fn new(
        chip: EccChip,
        mut layouter: impl Layouter<C::Base>,
        value: Option<C>,
    ) -> Result<Self, Error> {
        let point = chip.witness_point(&mut layouter, value);
        point.map(|inner| Point { chip, inner })
    }

    /// Constrains this point to be equal in value to another point.
    pub fn constrain_equal(
        &self,
        mut layouter: impl Layouter<C::Base>,
        other: &Self,
    ) -> Result<(), Error> {
        self.chip
            .constrain_equal(&mut layouter, &self.inner, &other.inner)
    }

    /// Returns the inner point.
    pub fn inner(&self) -> &EccChip::Point {
        &self.inner
    }

    /// Extracts the x-coordinate of a point.
    pub fn extract_p(&self) -> X<C, EccChip> {
        X::from_inner(self.chip.clone(), EccChip::extract_p(&self.inner).clone())
    }

    /// Wraps the given point (obtained directly from an instruction) in a gadget.
    pub fn from_inner(chip: EccChip, inner: EccChip::Point) -> Self {
        Point { chip, inner }
    }

    /// Returns `self + other` using complete addition.
    pub fn add(&self, mut layouter: impl Layouter<C::Base>, other: &Self) -> Result<Self, Error> {
        assert_eq!(self.chip, other.chip);
        self.chip
            .add(&mut layouter, &self.inner, &other.inner)
            .map(|inner| Point {
                chip: self.chip.clone(),
                inner,
            })
    }

    /// Returns `self + other` using incomplete addition.
    pub fn add_incomplete(
        &self,
        mut layouter: impl Layouter<C::Base>,
        other: &Self,
    ) -> Result<Self, Error> {
        assert_eq!(self.chip, other.chip);
        self.chip
            .add_incomplete(&mut layouter, &self.inner, &other.inner)
            .map(|inner| Point {
                chip: self.chip.clone(),
                inner,
            })
    }

    /// Returns `[by] self`.
    pub fn mul(
        &self,
        mut layouter: impl Layouter<C::Base>,
        by: &EccChip::Var,
    ) -> Result<(Self, ScalarVar<C, EccChip>), Error> {
        self.chip
            .mul(&mut layouter, by, &self.inner)
            .map(|(point, scalar)| {
                (
                    Point {
                        chip: self.chip.clone(),
                        inner: point,
                    },
                    ScalarVar {
                        chip: self.chip.clone(),
                        inner: scalar,
                    },
                )
            })
    }
}

/// The affine short Weierstrass x-coordinate of an elliptic curve point over the
/// given curve.
#[derive(Debug)]
pub struct X<C: CurveAffine, EccChip: EccInstructions<C> + Clone + Debug + Eq> {
    chip: EccChip,
    inner: EccChip::X,
}

impl<C: CurveAffine, EccChip: EccInstructions<C> + Clone + Debug + Eq> X<C, EccChip> {
    /// Wraps the given x-coordinate (obtained directly from an instruction) in a gadget.
    pub fn from_inner(chip: EccChip, inner: EccChip::X) -> Self {
        X { chip, inner }
    }

    /// Returns the inner x-coordinate.
    pub fn inner(&self) -> &EccChip::X {
        &self.inner
    }
}

/// A constant elliptic curve point over the given curve, for which window tables have
/// been provided to make scalar multiplication more efficient.
///
/// Used in scalar multiplication with full-width scalars.
#[derive(Clone, Debug)]
pub struct FixedPoint<C: CurveAffine, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    chip: EccChip,
    inner: EccChip::FixedPoints,
}

impl<C: CurveAffine, EccChip> FixedPoint<C, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    #[allow(clippy::type_complexity)]
    /// Returns `[by] self`.
    pub fn mul(
        &self,
        mut layouter: impl Layouter<C::Base>,
        by: Option<C::Scalar>,
    ) -> Result<(Point<C, EccChip>, ScalarFixed<C, EccChip>), Error> {
        self.chip
            .mul_fixed(&mut layouter, by, &self.inner)
            .map(|(point, scalar)| {
                (
                    Point {
                        chip: self.chip.clone(),
                        inner: point,
                    },
                    ScalarFixed {
                        chip: self.chip.clone(),
                        inner: scalar,
                    },
                )
            })
    }

    /// Wraps the given fixed base (obtained directly from an instruction) in a gadget.
    pub fn from_inner(chip: EccChip, inner: EccChip::FixedPoints) -> Self {
        FixedPoint { chip, inner }
    }
}

/// A constant elliptic curve point over the given curve, used in scalar multiplication
/// with a base field element
#[derive(Clone, Debug)]
pub struct FixedPointBaseField<C: CurveAffine, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    chip: EccChip,
    inner: EccChip::FixedPointsBaseField,
}

impl<C: CurveAffine, EccChip> FixedPointBaseField<C, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    #[allow(clippy::type_complexity)]
    /// Returns `[by] self`.
    pub fn mul(
        &self,
        mut layouter: impl Layouter<C::Base>,
        by: EccChip::Var,
    ) -> Result<Point<C, EccChip>, Error> {
        self.chip
            .mul_fixed_base_field_elem(&mut layouter, by, &self.inner)
            .map(|inner| Point {
                chip: self.chip.clone(),
                inner,
            })
    }

    /// Wraps the given fixed base (obtained directly from an instruction) in a gadget.
    pub fn from_inner(chip: EccChip, inner: EccChip::FixedPointsBaseField) -> Self {
        FixedPointBaseField { chip, inner }
    }
}

/// A constant elliptic curve point over the given curve, used in scalar multiplication
/// with a short signed exponent
#[derive(Clone, Debug)]
pub struct FixedPointShort<C: CurveAffine, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    chip: EccChip,
    inner: EccChip::FixedPointsShort,
}

impl<C: CurveAffine, EccChip> FixedPointShort<C, EccChip>
where
    EccChip: EccInstructions<C> + Clone + Debug + Eq,
{
    #[allow(clippy::type_complexity)]
    /// Returns `[by] self`.
    pub fn mul(
        &self,
        mut layouter: impl Layouter<C::Base>,
        magnitude_sign: (EccChip::Var, EccChip::Var),
    ) -> Result<(Point<C, EccChip>, ScalarFixedShort<C, EccChip>), Error> {
        self.chip
            .mul_fixed_short(&mut layouter, magnitude_sign, &self.inner)
            .map(|(point, scalar)| {
                (
                    Point {
                        chip: self.chip.clone(),
                        inner: point,
                    },
                    ScalarFixedShort {
                        chip: self.chip.clone(),
                        inner: scalar,
                    },
                )
            })
    }

    /// Wraps the given fixed base (obtained directly from an instruction) in a gadget.
    pub fn from_inner(chip: EccChip, inner: EccChip::FixedPointsShort) -> Self {
        FixedPointShort { chip, inner }
    }
}

#[cfg(test)]
mod tests {
    use group::{prime::PrimeCurveAffine, Curve, Group};

    use halo2::{
        circuit::{Layouter, SimpleFloorPlanner},
        dev::MockProver,
        plonk::{Circuit, ConstraintSystem, Error},
    };
    use pasta_curves::pallas;

    use super::chip::{EccChip, EccConfig};
    use crate::circuit::gadget::utilities::lookup_range_check::LookupRangeCheckConfig;

    struct MyCircuit {}

    #[allow(non_snake_case)]
    impl Circuit<pallas::Base> for MyCircuit {
        type Config = EccConfig;
        type FloorPlanner = SimpleFloorPlanner;

        fn without_witnesses(&self) -> Self {
            MyCircuit {}
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
            let lookup_table = meta.lookup_table_column();
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
            // Shared fixed column for loading constants
            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            let range_check = LookupRangeCheckConfig::configure(meta, advices[9], lookup_table);
            EccChip::configure(meta, advices, lagrange_coeffs, range_check)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), Error> {
            let chip = EccChip::construct(config.clone());

            // Load 10-bit lookup table. In the Action circuit, this will be
            // provided by the Sinsemilla chip.
            config.lookup_config.load(&mut layouter)?;

            // Generate a random point P
            let p_val = pallas::Point::random(rand::rngs::OsRng).to_affine(); // P
            let p = super::Point::new(chip.clone(), layouter.namespace(|| "P"), Some(p_val))?;
            let p_neg = -p_val;
            let p_neg = super::Point::new(chip.clone(), layouter.namespace(|| "-P"), Some(p_neg))?;

            // Generate a random point Q
            let q_val = pallas::Point::random(rand::rngs::OsRng).to_affine(); // Q
            let q = super::Point::new(chip.clone(), layouter.namespace(|| "Q"), Some(q_val))?;

            // Make sure P and Q are not the same point.
            assert_ne!(p_val, q_val);

            // Generate a (0,0) point to be used in other tests.
            let zero = {
                super::Point::new(
                    chip.clone(),
                    layouter.namespace(|| "identity"),
                    Some(pallas::Affine::identity()),
                )?
            };

            // Test complete addition
            {
                super::chip::add::tests::test_add(
                    chip.clone(),
                    layouter.namespace(|| "complete addition"),
                    &zero,
                    p_val,
                    &p,
                    q_val,
                    &q,
                    &p_neg,
                )?;
            }

            // Test incomplete addition
            {
                super::chip::add_incomplete::tests::test_add_incomplete(
                    chip.clone(),
                    layouter.namespace(|| "incomplete addition"),
                    &zero,
                    p_val,
                    &p,
                    q_val,
                    &q,
                    &p_neg,
                )?;
            }

            // Test variable-base scalar multiplication
            {
                super::chip::mul::tests::test_mul(
                    chip.clone(),
                    layouter.namespace(|| "variable-base scalar mul"),
                    &zero,
                    &p,
                    p_val,
                )?;
            }

            // Test full-width fixed-base scalar multiplication
            {
                super::chip::mul_fixed::full_width::tests::test_mul_fixed(
                    chip.clone(),
                    layouter.namespace(|| "full-width fixed-base scalar mul"),
                )?;
            }

            // Test signed short fixed-base scalar multiplication
            {
                super::chip::mul_fixed::short::tests::test_mul_fixed_short(
                    chip.clone(),
                    layouter.namespace(|| "signed short fixed-base scalar mul"),
                )?;
            }

            // Test fixed-base scalar multiplication with a base field element
            {
                super::chip::mul_fixed::base_field_elem::tests::test_mul_fixed_base_field(
                    chip,
                    layouter.namespace(|| "fixed-base scalar mul with base field element"),
                )?;
            }

            Ok(())
        }
    }

    #[test]
    fn ecc_chip() {
        let k = 13;
        let circuit = MyCircuit {};
        let prover = MockProver::run(k, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()))
    }

    #[cfg(feature = "dev-graph")]
    #[test]
    fn print_ecc_chip() {
        use plotters::prelude::*;

        let root = BitMapBackend::new("ecc-chip-layout.png", (1024, 7680)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.titled("Ecc Chip Layout", ("sans-serif", 60)).unwrap();

        let circuit = MyCircuit {};
        halo2::dev::CircuitLayout::default()
            .render(13, &circuit, &root)
            .unwrap();
    }
}
