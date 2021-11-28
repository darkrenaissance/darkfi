use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::{arithmetic::Field, pallas};
use rand::RngCore;

use crate::crypto::{constants::OrchardFixedBases, util::mod_r_p};

#[derive(Clone, Debug)]
pub struct Keypair {
    secret: pallas::Base,
    public: pallas::Point,
}

impl Keypair {
    fn derive_pub(s: pallas::Base) -> pallas::Point {
        OrchardFixedBases::NullifierK.generator() * mod_r_p(s)
    }

    pub fn new(secret: pallas::Base) -> Self {
        let public = Keypair::derive_pub(secret);
        Keypair { secret, public }
    }

    pub fn random(mut rng: impl RngCore) -> Self {
        let secret = pallas::Base::random(&mut rng);
        let public = Keypair::derive_pub(secret);
        Keypair { secret, public }
    }
}
