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
        constants::MERKLE_DEPTH_ORCHARD,
        pedersen::{pedersen_commitment_base, pedersen_commitment_u64},
        poseidon_hash,
        util::mod_r_p,
        Keypair, MerkleNode, Nullifier, SecretKey,
    },
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{
        arithmetic::CurveAffine,
        group::{ff::PrimeField, Curve},
        pallas,
    },
};
use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::arithmetic::Field;
use log::info;
use rand::{rngs::OsRng, thread_rng, Rng};

use super::{
    utils::fbig2base, Float10, EPOCH_LENGTH, LOTTERY_HEAD_START, P, PRF_NULLIFIER_PREFIX,
    RADIX_BITS, REWARD,
};
use crate::{
    crypto::{
        coin::{Coin, OwnCoin},
        leadcoin::LeadCoin,
        note::Note,
        types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValueBlind},
    },
    wallet::walletdb::WalletDb,
    Result,
};

const MERKLE_DEPTH: u8 = MERKLE_DEPTH_ORCHARD as u8;

/// Retrieve previous epoch competing coins frequency.
fn get_frequency() -> Float10 {
    //TODO: Actually retrieve frequency of coins from the previous epoch.
    let one: Float10 = Float10::from_str_native("1").unwrap().with_precision(*RADIX_BITS).value();
    let two: Float10 = Float10::from_str_native("2").unwrap().with_precision(*RADIX_BITS).value();
    one / two
}

/// Calculate nodes total stake for specific epoch and slot.
fn total_stake(_epoch: u64, _slot: u64) -> u64 {
    // TODO: fix this
    //(epoch * *EPOCH_LENGTH + slot + 1) * *REWARD
    *REWARD
}

/// Generate epoch competing coins.
pub fn create_epoch_coins(
    eta: pallas::Base,
    owned: &Vec<OwnCoin>,
    epoch: u64,
    slot: u64,
) -> Vec<Vec<LeadCoin>> {
    info!("Creating coins for epoch: {}", epoch);

    // Retrieve previous epoch competing coins frequency
    let frequency = get_frequency().with_precision(*RADIX_BITS).value();
    info!("Previous epoch frequency: {}", frequency);

    // Generating sigmas
    let total_stake = total_stake(epoch, slot); // only used for fine tunning
    info!("Node total stake: {}", total_stake);
    let one: Float10 = Float10::from_str_native("1").unwrap().with_precision(*RADIX_BITS).value();
    let two: Float10 = Float10::from_str_native("2").unwrap().with_precision(*RADIX_BITS).value();
    let field_p = Float10::from_str_native(*P).unwrap().with_precision(*RADIX_BITS).value();
    let total_sigma = Float10::try_from(total_stake).unwrap().with_precision(*RADIX_BITS).value();
    let x = one - frequency;
    info!("x: {}", x);
    let c = x.ln();
    info!("c: {}", c);
    let sigma1_fbig = c.clone() / total_sigma.clone() * field_p.clone();
    info!("sigma1: {}", sigma1_fbig);
    let sigma1: pallas::Base = fbig2base(sigma1_fbig);
    info!("sigma1 base: {:?}", sigma1);
    let sigma2_fbig = (c / total_sigma).powf(two.clone()) * (field_p / two);
    info!("sigma2: {}", sigma2_fbig);
    let sigma2: pallas::Base = fbig2base(sigma2_fbig);
    info!("sigma2 base: {:?}", sigma2);

    create_coins(eta, owned, sigma1, sigma2)
}

/// Generate coins for provided sigmas.
/// Note: the strategy here is single competing coin per slot.
fn create_coins(
    eta: pallas::Base,
    owned: &Vec<OwnCoin>,
    sigma1: pallas::Base,
    sigma2: pallas::Base,
) -> Vec<Vec<LeadCoin>> {
    let mut rng = thread_rng();
    let mut seeds: Vec<u64> = vec![];
    for _i in 0..*EPOCH_LENGTH {
        let rho: u64 = rng.gen();
        seeds.push(rho);
    }
    let (sks, root_sks, path_sks) = create_coins_sks();
    let mut tree_cm = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(*EPOCH_LENGTH as usize);
    // Leadcoins matrix were each row represents a slot and contains its competing coins.
    let mut coins: Vec<Vec<LeadCoin>> = vec![];

    // Use existing stake
    if !owned.is_empty() {
        for i in 0..*EPOCH_LENGTH {
            let index = i as usize;
            let mut slot_coins = vec![];
            for elem in owned {
                let coin = create_leadcoin(
                    eta,
                    sigma1,
                    sigma2,
                    elem.note.value,
                    index,
                    root_sks[index],
                    path_sks[index],
                    seeds[index],
                    sks[index],
                    &mut tree_cm,
                );
                slot_coins.push(coin);
            }
            coins.push(slot_coins);
            continue
        }
    } else {
        for i in 0..*EPOCH_LENGTH {
            let index = i as usize;
            // Compete with zero stake
            let coin = create_leadcoin(
                eta,
                sigma1,
                sigma2,
                *LOTTERY_HEAD_START,
                index,
                root_sks[index],
                path_sks[index],
                seeds[index],
                sks[index],
                &mut tree_cm,
            );
            coins.push(vec![coin]);
        }
    }
    coins
}

/// Generate epoch coins secret keys.
/// First slot coin secret key is sampled at random,
/// while the secret keys of the rest slots derive from previous slot secret.
/// Clarification:
///     sk[0] -> random,
///     sk[1] -> derive_function(sk[0]),
///     ...
///     sk[n] -> derive_function(sk[n-1]),
fn create_coins_sks() -> (Vec<SecretKey>, Vec<MerkleNode>, Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]>)
{
    let mut rng = thread_rng();
    let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(*EPOCH_LENGTH as usize);
    let mut sks: Vec<SecretKey> = vec![];
    let mut root_sks: Vec<MerkleNode> = vec![];
    let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
    let mut prev_sk_base: pallas::Base = pallas::Base::one();
    for _i in 0..*EPOCH_LENGTH {
        let base: pallas::Point = if _i == 0 {
            pedersen_commitment_u64(1, pallas::Scalar::random(&mut rng))
        } else {
            pedersen_commitment_u64(1, mod_r_p(prev_sk_base))
        };
        let coord = base.to_affine().coordinates().unwrap();
        let sk_x = *coord.x();
        let sk_y = *coord.y();
        let sk_coord_ar = [sk_x, sk_y];
        let sk_base: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(sk_coord_ar);
        sks.push(SecretKey::from(sk_base));
        prev_sk_base = sk_base;
        let sk_bytes = sk_base.to_repr();
        let node = MerkleNode::from_bytes(sk_bytes).unwrap();
        tree.append(&node.clone());
        let leaf_position = tree.witness();
        let root = tree.root(0).unwrap();
        let path = tree.authentication_path(leaf_position.unwrap(), &root).unwrap();
        root_sks.push(root);
        path_sks.push(path.as_slice().try_into().unwrap());
    }
    (sks, root_sks, path_sks)
}

/// Generate lead coin for provided sigmas and secret keys.
fn create_leadcoin(
    eta: pallas::Base,
    sigma1: pallas::Base,
    sigma2: pallas::Base,
    value: u64,
    i: usize,
    c_root_sk: MerkleNode,
    c_path_sk: [MerkleNode; MERKLE_DEPTH_ORCHARD],
    seed: u64,
    sk: SecretKey,
    tree_cm: &mut BridgeTree<MerkleNode, MERKLE_DEPTH>,
) -> LeadCoin {
    // keypair
    let keypair: Keypair = Keypair::new(sk);
    //random commitment blinding values
    let mut rng = thread_rng();
    let one = pallas::Base::one();
    let zero = pallas::Base::zero();
    let c_cm1_blind: DrkValueBlind = pallas::Scalar::random(&mut rng);
    let c_cm2_blind: DrkValueBlind = pallas::Scalar::random(&mut rng);

    let c_v = pallas::Base::from(value);
    // coin relative slot index in the epoch
    let c_sl = pallas::Base::from(u64::try_from(i).unwrap());
    //
    //let's assume it's sl for simplicity
    let c_tau = pallas::Base::from(u64::try_from(i).unwrap());
    //

    //let coin_pk_msg = [c_tau, c_root_sk.inner()];
    //let c_pk: pallas::Base = poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init().hash(coin_pk_msg);

    let c_pk: pallas::Point = keypair.public.inner();
    let c_pk_coord = c_pk.to_affine().coordinates().unwrap();
    let c_pk_x = c_pk_coord.x();
    let c_pk_y = c_pk_coord.y();
    info!("coin pk [{}] x: {:?}", i, c_pk_x);
    info!("coin pk [{}] y: {:?}", i, c_pk_y);

    let c_seed = pallas::Base::from(seed);
    let sn_msg = [c_seed, c_root_sk.inner(), zero, one];
    let c_sn: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<4>, 3, 2>::init()
            .hash(sn_msg);

    let coin_commit_msg_input =
        [pallas::Base::from(*PRF_NULLIFIER_PREFIX), *c_pk_x, *c_pk_y, c_v, c_seed, one];
    let coin_commit_msg: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<6>, 3, 2>::init()
            .hash(coin_commit_msg_input);
    let c_cm: pallas::Point = pedersen_commitment_base(coin_commit_msg, c_cm1_blind);
    let c_cm_coordinates = c_cm.to_affine().coordinates().unwrap();
    let c_cm_msg = [*c_cm_coordinates.x(), *c_cm_coordinates.y()];
    let c_cm_base: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
            .hash(c_cm_msg);
    let c_cm_node = MerkleNode::from(c_cm_base);
    tree_cm.append(&c_cm_node.clone());
    let leaf_position = tree_cm.witness().unwrap();
    let leaf_position_usize: usize = leaf_position.into();
    //info!("leaf position odd parity: {:?}", leaf_position.is_odd());
    let c_root_cm = tree_cm.root(0).unwrap();
    let c_cm_path = tree_cm.authentication_path(leaf_position, &c_root_cm).unwrap();

    /*
    let c_root_cm = {
        let mut current = MerkleNode::from(c_cm_base);
        let pos = leaf_position.unwrap();
        for (level, sibling) in c_cm_path.iter().enumerate() {
            let level = level as u8;
            current = if i & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };
    */

    let coin_nonce2_msg = [c_seed, c_root_sk.inner(), one, one];
    let c_seed2: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<4>, 3, 2>::init()
            .hash(coin_nonce2_msg);
    info!("coin2 seed [{}] : {:?}", i, c_seed2);
    let coin2_commit_msg_input =
        [pallas::Base::from(*PRF_NULLIFIER_PREFIX), *c_pk_x, *c_pk_y, c_v, c_seed2, one];
    let coin2_commit_msg: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<6>, 3, 2>::init()
            .hash(coin2_commit_msg_input);
    let c_cm2 = pedersen_commitment_base(coin2_commit_msg, c_cm2_blind);

    // election seeds
    let (y_mu, rho_mu) = create_coins_election_seeds(eta, c_sl);
    let coin = LeadCoin {
        value: Some(value),
        cm: Some(c_cm),
        cm2: Some(c_cm2),
        idx: u32::try_from(leaf_position_usize).unwrap(), //TODO should be abs slot
        sl: Some(c_sl),
        tau: Some(c_tau),
        nonce: Some(c_seed),
        nonce_cm: Some(c_seed2),
        sn: Some(c_sn),
        keypair: Some(keypair),
        root_cm: Some(c_root_cm.inner()),
        root_sk: Some(c_root_sk.inner()),
        path: Some(c_cm_path.as_slice().try_into().unwrap()),
        path_sk: Some(c_path_sk),
        c1_blind: Some(c_cm1_blind),
        c2_blind: Some(c_cm2_blind),
        y_mu: Some(y_mu),
        rho_mu: Some(rho_mu),
        sigma1: Some(sigma1),
        sigma2: Some(sigma2),
    };
    coin
}

fn create_coins_election_seeds(
    eta: pallas::Base,
    slot: pallas::Base,
) -> (pallas::Base, pallas::Base) {
    let election_seed_nonce: pallas::Base = pallas::Base::from(3);
    let election_seed_lead: pallas::Base = pallas::Base::from(22);

    // mu_rho
    let nonce_mu_msg = [election_seed_nonce, eta, slot];
    let nonce_mu: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<3>, 3, 2>::init()
            .hash(nonce_mu_msg);
    // mu_y
    let lead_mu_msg = [election_seed_lead, eta, slot];
    let lead_mu: pallas::Base =
        poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<3>, 3, 2>::init()
            .hash(lead_mu_msg);
    (lead_mu, nonce_mu)
}

/// Check that the provided participant/stakeholder coins win the slot lottery.
/// If the stakeholder have multiple competing winning coins, only the highest value coin is selected,
/// since the stakeholder can't give more than a proof per block(slot).
/// * `slot` - slot relative index
/// * `epoch_coins` - stakeholders epoch coins
/// Returns: (check: bool, idx: usize) where idx is the winning coin index
pub fn is_leader(slot: u64, epoch_coins: &Vec<Vec<LeadCoin>>) -> (bool, usize) {
    let slot_usize = slot as usize;
    info!("slot: {}, coins len: {}", slot, epoch_coins.len());
    assert!(slot_usize < epoch_coins.len());
    let competing_coins: &Vec<LeadCoin> = &epoch_coins[slot_usize];
    let mut won = false;
    let mut highest_stake = 0;
    let mut highest_stake_idx: usize = 0;
    for (winning_idx, coin) in competing_coins.iter().enumerate() {
        let y_exp = [coin.root_sk.unwrap(), coin.nonce.unwrap()];
        let y_exp_hash: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(y_exp);
        let y_coordinates = pedersen_commitment_base(y_exp_hash, mod_r_p(coin.y_mu.unwrap()))
            .to_affine()
            .coordinates()
            .unwrap();
        //
        let y_x: pallas::Base = *y_coordinates.x();
        let y_y: pallas::Base = *y_coordinates.y();
        let y_coord_arr = [y_x, y_y];
        let y: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(y_coord_arr);
        //
        let val_base = pallas::Base::from(coin.value.unwrap());
        let target_base =
            coin.sigma1.unwrap() * val_base + coin.sigma2.unwrap() * val_base * val_base;
        info!("y: {:?}", y);
        info!("T: {:?}", target_base);
        let first_winning = y < target_base;
        if first_winning && !won {
            highest_stake_idx = winning_idx;
        }
        won |= first_winning;
        if won && coin.value.unwrap() > highest_stake {
            highest_stake = coin.value.unwrap();
            highest_stake_idx = winning_idx;
        }
    }

    (won, highest_stake_idx)
}

/// Generate staking coins for provided wallet.
pub async fn generate_staking_coins(wallet: &WalletDb) -> Result<Vec<OwnCoin>> {
    let keypair = wallet.get_default_keypair().await?;
    let token_id = DrkTokenId::random(&mut OsRng);
    let value = 420;
    let serial = DrkSerial::random(&mut OsRng);
    let note = Note {
        serial,
        value,
        token_id,
        coin_blind: DrkCoinBlind::random(&mut OsRng),
        value_blind: DrkValueBlind::random(&mut OsRng),
        token_blind: DrkValueBlind::random(&mut OsRng),
        memo: vec![],
    };
    let coin = Coin(pallas::Base::random(&mut OsRng));
    let nullifier = Nullifier::from(poseidon_hash::<2>([keypair.secret.inner(), serial]));
    let leaf_position: incrementalmerkletree::Position = 0.into();
    let coin = OwnCoin { coin, note, secret: keypair.secret, nullifier, leaf_position };
    wallet.put_own_coin(coin.clone()).await?;

    Ok(vec![coin])
}
