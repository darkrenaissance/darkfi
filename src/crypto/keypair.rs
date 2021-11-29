use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::{arithmetic::Field, pallas};
use rand::RngCore;

use crate::crypto::{constants::OrchardFixedBases, util::mod_r_p};

#[derive(Clone, Debug)]
pub struct Keypair {
    pub secret: SecretKey,
    pub public: PublicKey,
}

impl Keypair {
    fn derive_public_key(s: SecretKey) -> PublicKey {
        PublicKey(OrchardFixedBases::NullifierK.generator() * mod_r_p(s.inner()))
    }

    pub fn new(secret: SecretKey) -> Self {
        let public = Keypair::derive_public_key(secret.clone());
        Keypair { secret, public }
    }

    pub fn random(mut rng: impl RngCore) -> Self {
        let secret = SecretKey::random(&mut rng);
        Keypair::new(secret)
    }
}

#[derive(Clone, Debug)]
pub struct SecretKey(pallas::Base);

impl SecretKey {
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn random(mut rng: impl RngCore) -> Self {
        let x = pallas::Base::random(&mut rng);
        SecretKey(x)
    }
}

#[derive(Clone, Debug)]
pub struct PublicKey(pallas::Point);

impl PublicKey {
    pub fn inner(&self) -> pallas::Point {
        self.0
    }
}
