# remarks on sapling for consensus

- both reward, and value are private witness in the contract, although reward is public value. either constrain reward in both mint, and reward contract, or use HE for validation.

- although secret key is enforced in the burn contract, in the mint contract pub_x, pub_y is loose, can be any value, shouldn't be used in seed

# issues

- constrain coin (if it's still going to be used) in reward contract so it can be validated that it's the same coin, and exist in the merkle tree

- constrain headstart in reward contract so it can be validated.

- lottery seed in reward contract, it should take deterministic nonce, and root of secret key, current implementation uses coin, and secret_key, which allow grinding attack by using different random blinds, same issue the secret key allow grinding attack because it's not enforced, and can't be constrained of course because it's private.
- even if we create tree for secret keys similar to crypsinous, it still can't be used alone, since root secret key will be the same, while it should be random, it should be concatenated with deterministic nonce.

## deterministic nonce
- serial number can be derived from previous serial number, again loose nonce allow grinding attack, by picking favouring seed, nonce pair for higher probability of winning.

# Plan of attack

- Reward and value:
    HE is already used to validate minted_coin_value = burnt_coin_value + reward using their value commitments, so no need to constrain it.
    Code block: https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/proposal_reward_v1.rs#L135-L143

- Headstart:
    Constrain headstart in reward proof and validators calculate it ad-hoc to verify it.

- Burnt coin constrain:
    Constrain burnt coin nullifier in reward proof, so we ensure same coin is used in burn and reward steps of the sequence.

- Lottery seed:
    Minted coin serial to be derived from burnt coin serial: minted_serial = poseidon_hash(NONCE_PREFIX, secret_key, burnt_coin_serial, ZERRO).
    Deterministic function code block: https://github.com/darkrenaissance/darkfi/blob/master/src/consensus/lead_coin.rs#L231-L240
    Use burnt coin serial to calculate lottery seed in proof: seed = poseidon_hash(SEED_PREFIX, burnt_serial, ZERO);
