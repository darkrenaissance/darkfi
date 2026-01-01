/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
use std::{io, io::Cursor};

#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;

use darkfi_sdk::pasta::{pallas, vesta};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_proofs::{
    helpers::SerdeFormat,
    plonk,
    plonk::{Circuit, SingleVerifier},
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite},
};
use rand::RngCore;

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

    pub fn write<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut params = vec![];
        self.params.write(&mut params)?;

        let mut vk = vec![];
        self.vk.write(&mut vk, SerdeFormat::RawBytes)?;

        let _ = writer.write(&(params.len() as u32).to_le_bytes())?;
        let _ = writer.write(&params)?;
        let _ = writer.write(&(vk.len() as u32).to_le_bytes())?;
        let _ = writer.write(&vk)?;

        Ok(())
    }

    pub fn read<R: io::Read, ConcreteCircuit: Circuit<pallas::Base>>(
        reader: &mut R,
        circuit: ConcreteCircuit,
    ) -> io::Result<Self> {
        // The format chosen in write():
        // [params.len()<u32>, params..., vk.len()<u32>, vk...]

        let mut params_len = [0u8; 4];
        reader.read_exact(&mut params_len)?;
        let params_len = u32::from_le_bytes(params_len) as usize;

        let mut params_buf = vec![0u8; params_len];
        reader.read_exact(&mut params_buf)?;

        assert!(params_buf.len() == params_len);

        let mut vk_len = [0u8; 4];
        reader.read_exact(&mut vk_len)?;
        let vk_len = u32::from_le_bytes(vk_len) as usize;

        let mut vk_buf = vec![0u8; vk_len];
        reader.read_exact(&mut vk_buf)?;

        assert!(vk_buf.len() == vk_len);

        let mut params_c = Cursor::new(params_buf);
        let params: Params<vesta::Affine> = Params::read(&mut params_c)?;

        let mut vk_c = Cursor::new(vk_buf);
        let vk: plonk::VerifyingKey<vesta::Affine> =
            plonk::VerifyingKey::read::<Cursor<Vec<u8>>, ConcreteCircuit>(
                &mut vk_c,
                SerdeFormat::RawBytes,
                circuit.params(),
            )?;

        Ok(Self { params, vk })
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

    pub fn write<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut params = vec![];
        self.params.write(&mut params)?;

        let mut pk = vec![];
        self.pk.write(&mut pk, SerdeFormat::RawBytes)?;

        let _ = writer.write(&(params.len() as u32).to_le_bytes())?;
        let _ = writer.write(&params)?;
        let _ = writer.write(&(pk.len() as u32).to_le_bytes())?;
        let _ = writer.write(&pk)?;

        Ok(())
    }

    pub fn read<R: io::Read, ConcreteCircuit: Circuit<pallas::Base>>(
        reader: &mut R,
        circuit: ConcreteCircuit,
    ) -> io::Result<Self> {
        let mut params_len = [0u8; 4];
        reader.read_exact(&mut params_len)?;
        let params_len = u32::from_le_bytes(params_len) as usize;

        let mut params_buf = vec![0u8; params_len];
        reader.read_exact(&mut params_buf)?;

        assert!(params_buf.len() == params_len);

        let mut pk_len = [0u8; 4];
        reader.read_exact(&mut pk_len)?;
        let pk_len = u32::from_le_bytes(pk_len) as usize;

        let mut pk_buf = vec![0u8; pk_len];
        reader.read_exact(&mut pk_buf)?;

        assert!(pk_buf.len() == pk_len);

        let mut params_c = Cursor::new(params_buf);
        let params: Params<vesta::Affine> = Params::read(&mut params_c)?;

        let mut pk_c = Cursor::new(pk_buf);
        let pk: plonk::ProvingKey<vesta::Affine> =
            plonk::ProvingKey::read::<Cursor<Vec<u8>>, ConcreteCircuit>(
                &mut pk_c,
                SerdeFormat::RawBytes,
                circuit.params(),
            )?;

        Ok(Self { params, pk })
    }
}

#[derive(Clone, Default, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Proof(Vec<u8>);

impl AsRef<[u8]> for Proof {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl core::fmt::Debug for Proof {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Proof({:?})", self.0)
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
