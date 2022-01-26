use std::io;

// TODO: Alias vesta::Affine to something
use halo2::{
    plonk,
    plonk::Circuit,
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite},
};
use pasta_curves::vesta;

use crate::{
    crypto::types::*,
    util::serial::{encode_with_size, Decodable, Encodable, ReadExt, VarInt},
    Result,
};

#[derive(Debug)]
pub struct VerifyingKey {
    pub params: Params<vesta::Affine>,
    pub vk: plonk::VerifyingKey<vesta::Affine>,
}

impl VerifyingKey {
    pub fn build(k: u32, c: &impl Circuit<DrkCircuitField>) -> Self {
        let params = Params::new(k);
        let vk = plonk::keygen_vk(&params, c).unwrap();
        VerifyingKey { params, vk }
    }
}

#[derive(Debug)]
pub struct ProvingKey {
    pub params: Params<vesta::Affine>,
    pub pk: plonk::ProvingKey<vesta::Affine>,
}

impl ProvingKey {
    pub fn build(k: u32, c: &impl Circuit<DrkCircuitField>) -> Self {
        let params = Params::new(k);
        let vk = plonk::keygen_vk(&params, c).unwrap();
        let pk = plonk::keygen_pk(&params, vk, c).unwrap();
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
    ) -> std::result::Result<Self, plonk::Error> {
        let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);

        plonk::create_proof(&pk.params, &pk.pk, circuits, &[&[pubinputs]], &mut transcript)?;

        Ok(Proof(transcript.finalize()))
    }

    pub fn verify(
        &self,
        vk: &VerifyingKey,
        pubinputs: &[DrkCircuitField],
    ) -> std::result::Result<(), plonk::Error> {
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

impl Encodable for Proof {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_with_size(self.as_ref(), s)
    }
}

impl Decodable for Proof {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let len = VarInt::decode(&mut d)?.0 as usize;
        let mut r = vec![0u8; len];
        d.read_slice(&mut r)?;
        Ok(Proof::new(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{keypair::PublicKey, mint_proof::create_mint_proof},
        zk::circuit::MintContract,
    };
    use halo2::arithmetic::Field;
    use rand::rngs::OsRng;

    #[test]
    fn test_proof_serialization() -> Result<()> {
        let value = 110_u64;
        let token_id = DrkTokenId::from(42);
        let value_blind = DrkValueBlind::random(&mut OsRng);
        let token_blind = DrkValueBlind::random(&mut OsRng);
        let serial = DrkSerial::random(&mut OsRng);
        let coin_blind = DrkCoinBlind::random(&mut OsRng);
        let public_key = PublicKey::random(&mut OsRng);

        let pk = ProvingKey::build(11, MintContract::default());
        let (proof, _) = create_mint_proof(
            &pk,
            value,
            token_id,
            value_blind,
            token_blind,
            serial,
            coin_blind,
            public_key,
        )?;

        let mut buf = vec![];
        proof.encode(&mut buf)?;
        let deserialized_proof: Proof = Decodable::decode(&mut buf.as_slice())?;
        assert_eq!(proof.as_ref(), deserialized_proof.as_ref());

        Ok(())
    }
}
