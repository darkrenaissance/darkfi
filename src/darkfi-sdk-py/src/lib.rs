use std::ops::{Add, Deref, Mul};

use darkfi_sdk::{
    crypto::{
        constants::{
            fixed_bases::{VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_V_BYTES},
            NullifierK,
        },
        pallas,
        pasta_prelude::{Field, PrimeField},
        poseidon_hash,
        util::mod_r_p,
        MerkleNode, ValueCommit,
    },
    incrementalmerkletree::Hashable,
    pasta::{
        arithmetic::{CurveAffine, CurveExt},
        group::{Curve, Group},
    },
};
use halo2_gadgets::ecc::chip::FixedPoint;
use pyo3::prelude::*;
use rand::rngs::OsRng;

/// This represents an element of $\mathbb{F}_p$ where
///
/// `p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`
///
/// is the base field of the Pallas curve.
// The internal representation of this type is four 64-bit unsigned
// integers in little-endian order. `Fp` values are always in
// Montgomery form; i.e., Fp(a) = aR mod p, with R = 2^256.
#[pyclass]
#[derive(Clone, Debug)]
struct Base(pallas::Base);

#[pymethods]
impl Base {
    /// For now, we work with 128 bits in Python.
    /// Becasue pallas::Base has nice debug formatting for it.
    /// TODO: change it to from_raw
    #[new]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Base::from_u128(v))
    }

    #[staticmethod]
    fn random() -> Self {
        Self(pallas::Base::random(&mut OsRng))
    }

    #[staticmethod]
    fn modulus() -> String {
        pallas::Base::MODULUS.to_string()
    }

    #[staticmethod]
    fn zero() -> Self {
        Self(pallas::Base::zero())
    }

    #[staticmethod]
    fn one() -> Self {
        Self(pallas::Base::one())
    }

    #[staticmethod]
    fn poseidon_hash(messages: Vec<&PyCell<Self>>) -> Self {
        let l = messages.len();
        let messages: Vec<pallas::Base> = messages.iter().map(|m| m.borrow().deref().0).collect();
        // TODO: is there a more idomatic way?
        if l == 1 {
            let m: [pallas::Base; 1] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 2 {
            let m: [pallas::Base; 2] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 3 {
            let m: [pallas::Base; 3] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 4 {
            let m: [pallas::Base; 4] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 5 {
            let m: [pallas::Base; 5] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 6 {
            let m: [pallas::Base; 6] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 7 {
            let m: [pallas::Base; 7] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 8 {
            let m: [pallas::Base; 8] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 9 {
            let m: [pallas::Base; 9] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 10 {
            let m: [pallas::Base; 10] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 11 {
            let m: [pallas::Base; 11] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 12 {
            let m: [pallas::Base; 12] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 13 {
            let m: [pallas::Base; 13] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 14 {
            let m: [pallas::Base; 14] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 15 {
            let m: [pallas::Base; 15] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 16 {
            let m: [pallas::Base; 16] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else {
            panic!("Messages length violation, must be: 1 <= len <= 16");
        }
    }

    fn __repr__(&self) -> String {
        format!("Base({:?})", self.0)
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(self.0.add(&rhs.0))
    }

    fn sub(&self, rhs: &Self) -> Self {
        Self(self.0.sub(&rhs.0))
    }

    fn double(&self) -> Self {
        Self(self.0.double())
    }

    fn mul(&self, rhs: &Self) -> Self {
        Self(self.0.mul(&rhs.0))
    }

    fn neg(&self) -> Self {
        Self(self.0.neg())
    }

    fn square(&self) -> Self {
        Self(self.0.square())
    }

    /// pos(ition) encodes the left/right position on each level
    /// path is the the silbling on each level
    fn merkle_root(&self, pos: u64, path: Vec<&PyCell<Base>>) -> Self {
        // TOOD: consider adding length check, for pos and path, for extra defensiness
        let mut current = MerkleNode::new(self.0);
        for (level, sibling) in path.iter().enumerate() {
            let level = level as u8;
            let sibling = MerkleNode::new(sibling.borrow().deref().0);
            current = if pos & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, &sibling)
            } else {
                MerkleNode::combine(level.into(), &sibling, &current)
            };
        }
        let root = current.inner();
        Self(root)
    }
}
/// Wrapper around poseidon in `halo2_gadgets`
// pub fn poseidon_hash<const N: usize>(messages: [pallas::Base; N]) -> pallas::Base {
//     poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<N>, 3, 2>::init()
//         .hash(messages)
// }

// Why Scalar field is from the field vesta curve is defined over?
/// This represents an element of $\mathbb{F}_q$ where
///
/// `q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001`
///
/// is the base field of the Vesta curve.
// The internal representation of this type is four 64-bit unsigned
// integers in little-endian order. `Fq` values are always in
// Montgomery form; i.e., Fq(a) = aR mod q, with R = 2^256.
#[pyclass]
struct Scalar(pallas::Scalar);

#[pymethods]
impl Scalar {
    /// TODO: change it to from_raw
    #[new]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Scalar::from_u128(v))
    }

    #[staticmethod]
    fn random() -> Self {
        Self(pallas::Scalar::random(&mut OsRng))
    }

    #[staticmethod]
    fn modulus() -> String {
        pallas::Scalar::MODULUS.to_string()
    }

    #[staticmethod]
    fn zero() -> Self {
        Self(pallas::Scalar::zero())
    }

    #[staticmethod]
    fn one() -> Self {
        Self(pallas::Scalar::one())
    }

    fn __repr__(&self) -> String {
        format!("Scalar({:?})", self.0)
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(self.0.add(&rhs.0))
    }

    fn sub(&self, rhs: &Self) -> Self {
        Self(self.0.sub(&rhs.0))
    }

    fn double(&self) -> Self {
        Self(self.0.double())
    }

    fn mul(&self, rhs: &Self) -> Self {
        Self(self.0.mul(&rhs.0))
    }

    fn neg(&self) -> Self {
        Self(self.0.neg())
    }

    fn square(&self) -> Self {
        Self(self.0.square())
    }
}

#[pyclass]
struct Point(pallas::Point);

#[pymethods]
impl Point {
    #[staticmethod]
    fn identity() -> Self {
        Self(pallas::Point::identity())
    }

    #[staticmethod]
    fn generator() -> Self {
        Self(pallas::Point::generator())
    }

    fn to_affine(&self) -> Affine {
        Affine(self.0.to_affine())
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(self.0.add(rhs.0))
    }

    fn mul(&self, scalar: &Scalar) -> Self {
        Self(self.0.mul(scalar.0))
    }

    fn mul_base(&self, value: &Base) -> Self {
        let v = NullifierK.generator();
        Self(v * mod_r_p(value.0))
    }

    fn mul_short(&self, value: u64) -> Self {
        // Why does v need to be a random element from EP?
        // Why not NullifierK.generator() or some other pre-determined generator?
        let hasher = ValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
        let v = hasher(&VALUE_COMMITMENT_V_BYTES);
        Self(v * mod_r_p(pallas::Base::from(value)))
    }
}

#[pyclass]
struct Affine(pallas::Affine);

#[pymethods]
impl Affine {
    fn coordinates(&self) -> (Base, Base) {
        let coords = self.0.coordinates().unwrap();
        (Base(*coords.x()), Base(*coords.y()))
    }
}

/// On how to do submodules: https://pyo3.rs/v0.18.3/module#python-submodules
/// Binding that comes with the bolierplate.
/// The #[pymodule] procedural macro takes care of exporting the initialization function of your module to Python.
#[pymodule]
fn darkfi_sdk_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Base>()?;
    m.add_class::<Scalar>()?;
    m.add_class::<Point>()?;
    m.add_class::<Affine>()?;
    Ok(())
}
