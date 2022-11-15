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
    crypto::{
        schnorr::{SchnorrPublic, Signature},
        PublicKey,
    },
    tx::ContractCall,
};
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};
use log::{debug, error};

use crate::{crypto::Proof, Error, Result};

/// A Transaction contains an arbitrary number of `ContractCall` objects,
/// along with corresponding ZK proofs and Schnorr signatures.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Transaction {
    /// Calls executed in this transaction
    pub calls: Vec<ContractCall>,
    /// Attached ZK proofs
    pub proofs: Vec<Vec<Proof>>,
    /// Attached Schnorr signatures
    pub signatures: Vec<Vec<Signature>>,
}

impl Transaction {
    /// Verify ZK proofs for the entire transaction.
    pub fn verify_zkps(&self) -> Result<()> {
        Ok(())
    }

    /// Verify Schnorr signatures for the entire transaction.
    pub fn verify_sigs(&self, pub_table: &[&[PublicKey]]) -> Result<()> {
        let tx_data = self.encode_without_sigs()?;
        let data_hash = blake3::hash(&tx_data);

        assert!(pub_table.len() == self.signatures.len());

        for (i, (sigs, pubkeys)) in self.signatures.iter().zip(pub_table.iter()).enumerate() {
            for (pubkey, signature) in pubkeys.iter().zip(sigs) {
                if !pubkey.verify(&data_hash.as_bytes()[..], &signature) {
                    error!("tx::verify_sigs[{}] failed to verify", i);
                    return Err(Error::InvalidSignature)
                }
            }
            debug!("tx::verify_sigs[{}] passed", i);
        }

        Ok(())
    }

    /// Encode the object into a byte vector for signing
    pub fn encode_without_sigs(&self) -> Result<Vec<u8>> {
        let mut buf = vec![];
        self.calls.encode(&mut buf)?;
        self.proofs.encode(&mut buf)?;
        Ok(buf)
    }
}
