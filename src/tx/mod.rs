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
        schnorr::{SchnorrPublic, SchnorrSecret, Signature},
        PublicKey, SecretKey,
    },
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};
use log::{debug, error};
use rand::{CryptoRng, RngCore};

use crate::{
    crypto::{proof::VerifyingKey, Proof},
    Error, Result, VerifyFailed,
};

macro_rules! zip {
    ($x:expr) => ($x);
    ($x:expr, $($y:expr), +) => (
        $x.iter().zip(zip!($($y), +))
    )
}

/// A Transaction contains an arbitrary number of `ContractCall` objects,
/// along with corresponding ZK proofs and Schnorr signatures.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
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
    pub fn verify_zkps(
        &self,
        verifying_keys: &[(String, VerifyingKey)],
        zkp_table: Vec<Vec<(String, Vec<pallas::Base>)>>,
    ) -> Result<()> {
        // TODO: Are we sure we should assert here?
        assert_eq!(self.calls.len(), self.proofs.len());
        assert_eq!(self.calls.len(), zkp_table.len());

        for (call, (proofs, pubvals)) in zip!(self.calls, self.proofs, zkp_table) {
            assert_eq!(proofs.len(), pubvals.len());

            for (i, (proof, (zk_ns, public_vals))) in proofs.iter().zip(pubvals.iter()).enumerate()
            {
                if let Some(vk) = verifying_keys.iter().find(|x| &x.0 == zk_ns) {
                    // We have a verifying key for this
                    debug!("public inputs: {:#?}", public_vals);
                    if let Err(e) = proof.verify(&vk.1, public_vals) {
                        error!("Failed verifying zk proof: {}", e);
                        return Err(VerifyFailed::ProofVerifyFailed(e.to_string()).into())
                    }
                } else {
                    return Err(VerifyFailed::ProofVerifyFailed(format!(
                        "Verifying key for {} circuit does not exist",
                        zk_ns
                    ))
                    .into())
                }
            }
        }

        Ok(())
    }

    /// Verify Schnorr signatures for the entire transaction.
    pub fn verify_sigs(&self, pub_table: Vec<Vec<PublicKey>>) -> Result<()> {
        let tx_data = self.encode_without_sigs()?;
        let data_hash = blake3::hash(&tx_data);
        debug!("tx.verify_sigs: data_hash: {:?}", data_hash.as_bytes());

        assert!(pub_table.len() == self.signatures.len());

        for (i, (sigs, pubkeys)) in self.signatures.iter().zip(pub_table.iter()).enumerate() {
            for (pubkey, signature) in pubkeys.iter().zip(sigs) {
                debug!("Verifying signature with public key: {}", pubkey);
                if !pubkey.verify(&data_hash.as_bytes()[..], &signature) {
                    error!("tx::verify_sigs[{}] failed to verify", i);
                    return Err(Error::InvalidSignature)
                }
            }
            debug!("tx::verify_sigs[{}] passed", i);
        }

        Ok(())
    }

    /// Create Schnorr signatures for the entire transaction.
    pub fn create_sigs(
        &self,
        rng: &mut (impl CryptoRng + RngCore),
        secret_keys: &[SecretKey],
    ) -> Result<Vec<Signature>> {
        let tx_data = self.encode_without_sigs()?;
        let data_hash = blake3::hash(&tx_data);
        debug!("tx.create_sigs: data_hash: {:?}", data_hash.as_bytes());

        let mut sigs = vec![];
        for secret in secret_keys {
            debug!("Creating signature with public key: {}", PublicKey::from_secret(*secret));
            let signature = secret.sign(rng, &data_hash.as_bytes()[..]);
            sigs.push(signature);
        }

        Ok(sigs)
    }

    /// Encode the object into a byte vector for signing
    pub fn encode_without_sigs(&self) -> Result<Vec<u8>> {
        let mut buf = vec![];
        self.calls.encode(&mut buf)?;
        self.proofs.encode(&mut buf)?;
        Ok(buf)
    }
}
