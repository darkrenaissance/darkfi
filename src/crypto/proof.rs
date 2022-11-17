/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_proofs::{
    plonk,
    plonk::{Circuit, SingleVerifier},
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite},
};
use pasta_curves::{pallas, vesta};
use rand::RngCore;

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
    pub fn build(k: u32, c: &impl Circuit<pallas::Base>) -> Self {
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
    pub fn build(k: u32, c: &impl Circuit<pallas::Base>) -> Self {
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
        circuits: &[impl Circuit<pallas::Base>],
        instances: &[pallas::Base],
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
        instances: &[pallas::Base],
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
    use crate::{crypto::mint_proof::create_mint_proof, zk::circuit::MintContract, Result};
    use darkfi_sdk::{
        crypto::{PublicKey, SecretKey, TokenId},
        pasta::{group::ff::Field, pallas},
    };
    use darkfi_serial::{Decodable, Encodable};
    use rand::rngs::OsRng;

    #[test]
    fn test_proof_serialization() -> Result<()> {
        let value = 110_u64;
        let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
        let value_blind = ValueBlind::random(&mut OsRng);
        let token_blind = ValueBlind::random(&mut OsRng);
        let serial = pallas::Base::random(&mut OsRng);
        let spend_hook = pallas::Base::random(&mut OsRng);
        let user_data = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);
        let public_key = PublicKey::from_secret(SecretKey::random(&mut OsRng));

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
