use halo2_proofs::{
    plonk,
    plonk::{Circuit, SingleVerifier},
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite},
};
use pasta_curves::vesta;
use rand::RngCore;

use crate::{
    crypto::types::DrkCircuitField,
    serial::{SerialDecodable, SerialEncodable},
};

// TODO: this API needs rework. It's not very good.
// keygen_pk() takes a VerifyingKey by value,
// yet ProvingKey also provides get_vk() -> &VerifyingKey
//
// Maybe we should just use the native halo2 types instead of wrapping them.
// We can avoid double creating the vk when we call VerifyingKey::build(), ProvingKey::build()

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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

#[derive(Clone, Default, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
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
        instances: &[DrkCircuitField],
        mut rng: impl RngCore,
    ) -> std::result::Result<Self, plonk::Error> {
        let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
        plonk::create_proof(
            &pk.params,
            &pk.pk,
            circuits,
            &[&[instances]],
            &mut rng,
            &mut transcript,
        )?;

        Ok(Proof(transcript.finalize()))
    }

    pub fn verify(
        &self,
        vk: &VerifyingKey,
        instances: &[DrkCircuitField],
    ) -> std::result::Result<(), plonk::Error> {
        let strategy = SingleVerifier::new(&vk.params);
        let mut transcript = Blake2bRead::init(&self.0[..]);

        plonk::verify_proof(&vk.params, &vk.vk, strategy, &[&[instances]], &mut transcript)
    }

    pub fn new(bytes: Vec<u8>) -> Self {
        Proof(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{
            keypair::PublicKey,
            mint_proof::create_mint_proof,
            types::{
                DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData, DrkValueBlind,
            },
        },
        serial::{Decodable, Encodable},
        zk::circuit::MintContract,
        Result,
    };
    use group::ff::Field;
    use rand::rngs::OsRng;

    #[test]
    fn test_proof_serialization() -> Result<()> {
        let value = 110_u64;
        let token_id = DrkTokenId::random(&mut OsRng);
        let value_blind = DrkValueBlind::random(&mut OsRng);
        let token_blind = DrkValueBlind::random(&mut OsRng);
        let serial = DrkSerial::random(&mut OsRng);
        let spend_hook = DrkSpendHook::random(&mut OsRng);
        let user_data = DrkUserData::random(&mut OsRng);
        let coin_blind = DrkCoinBlind::random(&mut OsRng);
        let public_key = PublicKey::random(&mut OsRng);

        let pk = ProvingKey::build(11, &MintContract::default());
        let (proof, _) = create_mint_proof(
            &pk,
            value,
            token_id,
            value_blind,
            token_blind,
            serial,
            spend_hook,
            user_data,
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
