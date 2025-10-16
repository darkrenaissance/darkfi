# Concepts

The smart contract deployment process consists of two steps that are
outlined below:

* **Deploy:** smart contract is initialized on the blockchain.
* **Lock:** smart contract is finalized and can't be modified
  further.

## Deploy

User creates a new smart contract and posts it on chain. The provided
`WASM` bincode will initialize all the database trees required by the
smart contract. The contract state definition consinst of the current
contract `WASM` bincode and its database opened trees.

The smart contract state definition can be updated, as long as its
unlocked.

### Smart Contract Status

* *Unlocked*: the smart contract state definition can be updated.
* *Locked*: the smart contract is finalized and no more changes are
   allowed.

## Lock

User can finalize their smart contract state definition on chain,
locking down the contract, preventing further state changes. This
action is irreversible and the smart contract state definitions cannot
be modified afterwards.
