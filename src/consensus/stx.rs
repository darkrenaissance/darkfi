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
    crypto::MerkleNode,
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
};

use crate::{
    crypto::{proof::VerifyingKey, Proof},
    Error, Result,
};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct TransferStx {
    /// sender's coin, or coin1_commitment in zk
    pub coin_commitment: pallas::Point,
    /// sender's coin pk
    pub coin_pk: pallas::Base,
    /// sender's coin sk's root
    pub coin_root_sk: MerkleNode,
    /// coin3_commitment in zk
    pub change_coin_commitment: pallas::Point,
    /// coin4_commitment in zk
    pub transfered_coin_commitment: pallas::Point,
    /// nullifiers coin1_nullifier
    pub nullifier: pallas::Base,
    /// sk coin pos
    pub tau: pallas::Base,
    /// root to coin's commitments
    pub root: MerkleNode,
    /// transfer proof
    pub proof: Proof,
}

impl TransferStx {
    /// verify the transfer proof.
    pub fn verify(&self, vk: VerifyingKey) -> Result<()> {
        if let Err(e) = self.proof.verify(&vk, &self.public_inputs()) {
            return Err(Error::TransferTxVerification)
        }
        Ok(())
    }

    /// arrange public inputs from Stxfer
    pub fn public_inputs(&self) -> Vec<pallas::Base> {
        let cm1 = self.coin_commitment.to_affine().coordinates().unwrap();
        let cm3 = self.change_coin_commitment.to_affine().coordinates().unwrap();
        let cm4 = self.transfered_coin_commitment.to_affine().coordinates().unwrap();
        vec![
            self.coin_pk,
            *cm1.x(),
            *cm1.y(),
            *cm3.x(),
            *cm3.y(),
            *cm4.x(),
            *cm4.y(),
            self.root.inner(),
            self.coin_root_sk.inner(),
            self.nullifier,
        ]
    }
}
