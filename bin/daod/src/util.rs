use halo2_gadgets::poseidon::primitives as poseidon;
use pasta_curves::pallas;

pub fn poseidon_hash<const N: usize>(messages: [pallas::Base; N]) -> pallas::Base {
    poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<N>, 3, 2>::init()
        .hash(messages)
}
