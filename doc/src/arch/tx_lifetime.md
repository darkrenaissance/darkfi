# Transactions

_(Temporary document, to be integrated into other docs)_

## Transaction behaviour

In our network context, we have two types of nodes.

1. Miner (`M`)
2. Spectator (`S`)

`S` acts as a relayer for transactions in order to help out
that transactions reach `M`.

To avoid spam attacks, `S` should keep $tx$ in their mempool for some
period of time, and then prune it.

## Ideal simulation with instant confirmation

The lifetime of a transaction $tx$ that passes verification and whose
state transition can be applied on top of the canonical (confirmed)
chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `S`
3. `S` validates $tx$ state transition
4. $tx$ enters `S` `mempool`
5. `S` broadcasts $tx$ to `M`
6. `M` validates $tx$ state transition
7. $tx$ enters `M` `mempool`
8. `M` validates all transactions in its `mempool` in sequence
9. `M` proposes a block confirmation containing $tx$
10. `M` writes the state transition update of $tx$ to their chain
11. `M` removes $tx$ from their `mempool`
12. `M` broadcasts the confirmed proposal
13. `S` receives the proposal and validates transactions
14. `S` writes the state updates to their chain
15. `S` removes $tx$ from their `mempool`

## Real-world simulation with non-instant confirmation

The lifetime of a transaction $tx$ that passes verification and whose
state transition is pending to be applied on top of the canonical
(confirmed) chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `S`
3. `S` validates $tx$ state transition
4. $tx$ enters `S` `mempool`
5. `S` broadcasts $tx$ to `M`
6. `M` validates $tx$ state transition
7. $tx$ enters `M` `mempool`
8. `M` proposes a block proposal containing $tx$
9. `M` proposes more block proposals
10. When proposals can be confirmed, `M` validates all their transactions
in sequence
11. `M` writes the state transition update of $tx$ to their chain
12. `M` removes $tx$ from their `mempool`
13. `M` broadcasts the confirmed proposals sequence
14. `S` receives the proposals sequence and validates transactions
15. `S` writes the state updates to their chain
16. `S` removes $tx$ from their `mempool`

## Real-world simulation with non-instant confirmation, forks and multiple `CP` nodes

The lifetime of a transaction $tx$ that passes verifications and whose
state transition is pending to be applied on top of the canonical
(confirmed) chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `S`
3. `S` validates $tx$ state transition against canonical chain state
4. $tx$ enters `S` `mempool`
5. `S` broadcasts $tx$ to `P`
6. `M` validates $tx$ state transition against all known fork states
7. $tx$ enters `M` `mempool`
8. `M` broadcasts $tx$ to rest `M` nodes
9. Block producer `SM` finds which fork to extend
10. `SM` validates all unproposed transactions in its `mempool` in
  sequence, extended fork state, discarding invalid
11. `SM` creates a block proposal containing $tx$ extending the fork
12. `M` receives block proposal and validates its transactions against
the extended fork state
13. `SM` proposes more block proposals extending a fork state
14. When a fork can be confirmed, `M` validates all its proposals
transactions in sequence, against canonical state
15. `M` writes the state transition update of $tx$ to their chain
16. `M` removes $tx$ from their `mempool`
17. `M` drop rest forks and keeps only the confirmed one
18. `M` broadcasts the confirmed proposals sequence
19. `S` receives the proposals sequence and validates transactions
20. `S` writes the state updates to their chain
21. `S` removes $tx$ from their `mempool`

`M` will keep $tx$ in its `mempool` as long as it is a valid state
transition for any fork(including canonical) or it get confirmed.

Unproposed transactions refers to all $tx$ not included in a proposal
of any fork.

If a fork that can be confirmed fails to validate all its
transactions(14), it should be dropped.

## The `Transaction` object

```rust
pub struct ContractCall {
    /// The contract ID to which the payload is fed to
    pub contract_id: ContractId,
    /// Arbitrary payload for the contract call
    pub payload: Vec<u8>,
}

pub struct Transaction {
    /// Calls executed in this transaction
    pub calls: Vec<ContractCall>,
    /// Attached ZK proofs
    pub proofs: Vec<Vec<Proof>>,
    /// Attached Schnorr signatures
    pub signatures: Vec<Vec<Signature>>,
}
```

A generic DarkFi transaction object is simply an array of smart
contract calls, along with attached ZK proofs and signatures needed
to properly verify the contracts' execution. A transaction can have
any number of calls, and proofs, provided it does not exhaust a set
gas limit.

In DarkFi, every operation is a smart contract. This includes payments,
which we'll explain in the following section.

## Payments

For A -> B payments in DarkFi we use the Sapling scheme that originates
from zcash. A payment transaction has a number of _inputs_ (which are
coins being burned/spent), and a number of _outputs_ (which are coins
being minted/created). An explanation for the ZK proofs for this scheme
can be found [here](../zkas/examples/sapling.md) under the Zkas section
of this book.

In code, the structs we use are the following:

```rust
pub struct MoneyTransferParams {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

pub struct Input {
    /// Pedersen commitment for the input's value
    pub value_commit: ValueCommit,
    /// Pedersen commitment for the input's token ID
    pub token_commit: ValueCommit,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Public key for the Schnorr signature
    pub signature_public: PublicKey,
}

pub struct Output {
    /// Pedersen commitment for the output's value
    pub value_commit: ValueCommit,
    /// Pedersen commitment for the output's token ID
    pub token_commit: ValueCommit,
    /// Minted coin: poseidon_hash(pubkey, value, token, serial, blind)
    pub coin: Coin,
    /// The encrypted note ciphertext
    pub encrypted_note: EncryptedNote,
}

pub struct EncryptedNote {
    pub ciphertext: Vec<u8>,
    pub ephemeral_key: PublicKey,
}

pub struct Note {
    /// Serial number of the coin, used to derive the nullifier
    pub serial: pallas::Base,
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: TokenId,
    /// Blinding factor for the value Pedersen commitment
    pub value_blind: ValueBlind,
    /// Blinding factor for the token ID Pedersen commitment
    pub token_blind: ValueBlind,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}
```

In the blockchain state, every minted coin must be added into a Merkle
tree of all existing coins. Once added, the new tree root is used to
prove existence of this coin when it's being spent.

Let's imagine a scenario where Alice has 100 ALICE tokens and wants to
send them to Bob. Alice would create an `Input` object using the info
she has of her coin. She has to derive a `nullifier` given her secret
key and the serial number of the coin, hash the coin bulla so she can
create a merkle path proof, and derive the value and token commitments
using the blinds.

```rust
let nullifier = poseidon_hash([alice_secret_key, serial]);
let signature_public = alice_secret_key * Generator;
let coin = poseidon_hash([signature_public, value, token_id, blind]);
let merkle_root = calculate_merkle_root(coin);
let value_commit = pedersen_commitment(value, value_blind);
let token_commit = pedersen_commitment(token_id, token_blind);
```

The values above, except `coin` become the public inputs for the `Burn`
ZK proof. If everything is correct, this allows Alice to spend her coin.
In DarkFi, the changes have to be atomic, so any payment transaction
that is burning some coins, has to mint new coins at the same time, and
no value must be lost, nor can the token ID change. We enforce this by
using Pedersen commitments.

Now that Alice has a valid `Burn` proof and can spend her coin, she can
mint a new coin for Bob.

```rust
let blind = pallas::Base::random();
let value_blind = ValueBlind::random();
let token_blind = ValueBlind::random();
let coin = poseidon_hash([bob_public, value, token_id, blind]);
let value_commit = pedersen_commitment(value, value_blind);
let token_commit = pedersen_commitment(token, token_blind);
```

`coin`, `value_commit`, and `token_commit` become the public inputs
for the `Mint` ZK proof. If this proof is valid, it creates a new coin
for Bob with the given parameters. Additionally, Alice would put the
values and blinds in a `Note` which is encrypted with Bob's public key
so only Bob is able to decrypt it. This `Note` has the necessary info
for him to further spend the coin he received.

At this point Alice should have 1 input and 1 output. The input is the
coin she burned, and the output is the coin she minted for Bob. Along
with this, she has two ZK proofs that prove creation of the input and
output. Now she can build a transaction object, and then use her secret
key she derived in the `Burn` proof to sign the transaction and publish
it to the blockchain.

The blockchain will execute the smart contract with the given payload
and verify that the Pedersen commitments match, that the nullifier has
not been published before, and also that the merkle authentication path
is valid and therefore the coin existed in a previous state. Outside of
the VM, the validator will also verify the signature(s) and the ZK
proofs. If this is valid, then Alice's coin is now burned and cannot be
used anymore. And since Alice also created an output for Bob, this new
coin is now added to the Merkle tree and is able to be spent by him.
Effectively this means that Alice has sent her tokens to Bob.
