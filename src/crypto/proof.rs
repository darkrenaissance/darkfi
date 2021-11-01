use pasta_curves::vesta;
// TODO: Alias vesta::Affine to something

use halo2::{
    plonk,
    plonk::Circuit,
    poly::commitment,
    transcript::{Blake2bRead, Blake2bWrite},
};

use crate::types::*;

#[derive(Debug)]
pub struct VerifyingKey {
    pub params: commitment::Params<vesta::Affine>,
    pub vk: plonk::VerifyingKey<vesta::Affine>,
}

impl VerifyingKey {
    pub fn build(k: u32, c: impl Circuit<DrkCircuitField>) -> Self {
        let params = commitment::Params::new(k);
        let vk = plonk::keygen_vk(&params, &c).unwrap();
        VerifyingKey { params, vk }
    }
}

#[derive(Debug)]
pub struct ProvingKey {
    pub params: commitment::Params<vesta::Affine>,
    pub pk: plonk::ProvingKey<vesta::Affine>,
}

impl ProvingKey {
    pub fn build(k: u32, c: impl Circuit<DrkCircuitField>) -> Self {
        let params = commitment::Params::new(k);
        let vk = plonk::keygen_vk(&params, &c).unwrap();
        let pk = plonk::keygen_pk(&params, vk, &c).unwrap();
        ProvingKey { params, pk }
    }
}

#[derive(Clone, Debug)]
pub struct Proof(Vec<u8>);

impl AsRef<[u8]> for Proof {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Proof {
    pub fn create(
        pk: &ProvingKey,
        circuits: &[impl Circuit<DrkCircuitField>],
        pubinputs: &[DrkCircuitField],
    ) -> Result<Self, plonk::Error> {
        let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);

        plonk::create_proof(
            &pk.params,
            &pk.pk,
            circuits,
            &[&[pubinputs]],
            &mut transcript,
        )?;

        Ok(Proof(transcript.finalize()))
    }

    pub fn verify(
        &self,
        vk: &VerifyingKey,
        pubinputs: &[DrkCircuitField],
    ) -> Result<(), plonk::Error> {
        let msm = vk.params.empty_msm();
        let mut transcript = Blake2bRead::init(&self.0[..]);
        let guard = plonk::verify_proof(&vk.params, &vk.vk, msm, &[&[pubinputs]], &mut transcript)?;
        let msm = guard.clone().use_challenges();

        if msm.eval() {
            Ok(())
        } else {
            Err(plonk::Error::ConstraintSystemFailure)
        }
    }

    pub fn new(bytes: Vec<u8>) -> Self {
        Proof(bytes)
    }
}
