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

use darkfi_sdk::{
    crypto::{schnorr::Signature, Keypair, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::error;

use crate::{
    crypto::proof::{Proof, VerifyingKey},
    Result,
};

// TODO: Replace 'Lead' terms with 'Producer' to make it more clear that
// we refer to block producer.
/// This struct represents [`Block`](super::Block) leader information used by the consensus protocol.
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct LeadInfo {
    /// Block producer signature
    pub signature: Signature,
    /// Block producer public_key
    pub public_key: PublicKey, // TODO: remove this(to be derived by proof)
    /// Block producer slot competing coins public inputs
    pub public_inputs: Vec<pallas::Base>,
    /// Response of global random oracle, or it's emulation.
    pub eta: [u8; 32],
    /// Leader NIZK proof
    pub proof: LeadProof,
    /// Slot offset block producer used
    pub offset: u64,
    /// Block producer leaders count
    pub leaders: u64,
}

impl Default for LeadInfo {
    /// Default LeadInfo used in genesis block generation
    fn default() -> Self {
        let keypair = Keypair::default();
        let signature = Signature::dummy();
        let public_inputs = vec![];
        let eta: [u8; 32] = *blake3::hash(b"let there be dark!").as_bytes();
        let proof = LeadProof::default();
        let offset = 0;
        let leaders = 0;
        Self { signature, public_key: keypair.public, public_inputs, eta, proof, offset, leaders }
    }
}

impl LeadInfo {
    pub fn new(
        signature: Signature,
        public_key: PublicKey,
        public_inputs: Vec<pallas::Base>,
        eta: [u8; 32],
        proof: LeadProof,
        offset: u64,
        leaders: u64,
    ) -> Self {
        Self { signature, public_key, public_inputs, eta, proof, offset, leaders }
    }
}

/// Wrapper over the Proof, for future additions.
#[derive(Default, Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct LeadProof {
    /// Leadership proof
    pub proof: Proof,
}

impl LeadProof {
    pub fn verify(&self, vk: &VerifyingKey, public_inputs: &[pallas::Base]) -> Result<()> {
        if let Err(e) = self.proof.verify(vk, public_inputs) {
            error!("Verification of consensus lead proof failed: {}", e);
            return Err(e.into())
        }

        Ok(())
    }
}

impl From<Proof> for LeadProof {
    fn from(proof: Proof) -> Self {
        Self { proof }
    }
}
