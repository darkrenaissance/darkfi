# Scheme

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

## Transfer

This function transfers value by burning a set of coins $𝐂$, and minting a
set of coins, such that the value spent and created are equal.

* Wallet:
  * Builder: `src/contract/money/src/client/transfer_v1/builder.rs`
  * Convenience methods: `src/contract/money/src/client/transfer_v1/mod.rs`
  * Build proofs: `src/contract/money/src/client/transfer_v1/proof.rs`
* WASM VM code: `src/contract/money/src/entrypoint/transfer_v1.rs`
* ZK proofs:
  * `src/contract/money/proof/burn_v1.zk`
  * `src/contract/money/proof/mint_v1.zk`

### Function Params

Let $\t{MoneyClearInput}, \t{MoneyInput}, \t{MoneyOutput}$
be defined as in [Inputs and Outputs](model.md#inputs-and-outputs).

Define the Money transfer function params
$$ \begin{aligned}
  𝐣 &∈ \t{MoneyClearInput}^* \\
  𝐢 &∈ \t{MoneyInput}^* \\
  𝐨 &∈ \t{MoneyOutput}^*
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model/mod.rs:money-params}}
```

### Contract Statement

Let $π_\t{mint}, π_\t{burn}$ be defined as in [ZK Proofs](#zk-proofs).

### ZK Proofs

#### `Mint_V1`

Using the `Mint_V1` circuit, we are able to create outputs
in our UTXO set. It is used along with the `Burn_V1` circuit in
`MoneyFunction::TransferV1` where we perform a payment to some address
on the network.

Denote this proof by $π_\t{mint}$.

**Circuit witnesses:**

* $P$ - Public key of the recipient which goes into the coin commitment (pallas curve point)
* $v$ - Value of the coin commitment (unsigned 64-bit integer)
* $t$ - Token ID of the coin commitment (pallas base field element)
* $s$ - Unique serial number of the coin commitment (pallas base field element)
* $h$ - Spend hook, allows composing this ZK proof to invoke other contracts (pallas base field element)
* $u$ - Data passed from this coin to the invoked contract (pallas base field element)
* $v_\t{blind}$ - Random blinding factor for a Pedersen commitment to $v$ (pallas scalar field element)
* $t_\t{blind}$ - Random blinding factor for a commitment to $t$ (pallas base field element)

**Circuit public inputs:**

* $C$ - Coin commitment
* $V$ - Pedersen commitment to $v$
* $T$ - Token ID commitment

**Circuit:**

$$ C = \text{PoseidonHash}(P, v, t, s, h, u) $$
$$ \text{RangeCheck}(64, v) $$
$$ V = vG + v_{\text{blind}}H $$
$$ T = \text{PoseidonHash}(t, t_{\text{blind}}) $$

$G$ and $H$ are constant well-known generators that are in the codebase
as `VALUE_COMMIT_VALUE` and `VALUE_COMMIT_RANDOM`:


* `src/sdk/src/crypto/constants/fixed_bases/value_commit_v.rs`
* `src/sdk/src/crypto/constants/fixed_bases/value_commit_r.rs`

### `Burn_V1`

Using the `Burn_V1` circuit, we are able to create inputs in
our UTXO set. It is used along with the `Mint_V1` circuit in
`MoneyFunction::TransferV1` where we perform a payment to some address
on the network.

Denote this proof by $π_\t{burn}$.

**Circuit witnesses:**

* $v$ - Value of the coin being spent (unsigned 64-bit integer)
* $t$ - Token ID of the coin being spent (pallas curve base field element)
* $v_{\text{blind}}$ - Random blinding factor for a Pedersen commitment to $v$ (pallas scalar field element)
* $t_{\text{blind}}$ - Random blinding factor for a commitment to $t$ (pallas base field element)
* $s$ - Unique serial number of the coin commitment (pallas base field element)
* $h$ - Spend hook, allows composing this ZK proof to invoke other contracts (pallas base field element)
* $u$ - Data passed from this coin to the invoked contract (pallas base field element)
* $u_{\text{blind}}$ - Blinding factor for encrypting $u$ (pallas base field element)
* $x$ - Secret key used to derive $N$ (nullifier) and $P$ (public key) from the coin $C$ (pallas base field element)
* $l$ - Leaf position of $C$ in the Merkle tree of all coin commitments (unsigned 32-bit integer)
* $p$ - Merkle path to the coin $C$ in the Merkle tree (array of 32 pallas base field elements)
* $z$ - Secret key used to derive public key for the tx signature $Z$

**Circuit public inputs:**

* $N$ - Published nullifier to prevent double spending
* $V$ - Pedersen commitment to $v$
* $T$ - Token ID commitment
* $R$ - Merkle root calculated from $l$ and $p$
* $U$ - Commitment to $u$
* $h$ - Spend hook
* $Z$ - Public key derived from $z$ used for transaction signing

**Circuit:**

$$ N = \text{PoseidonHash}(x, s) $$
$$ V = vG + v_{\text{blind}}H $$
$$ T = \text{PoseidonHash}(t, t_{\text{blind}}) $$
$$ P = xK $$
$$ C = \text{PoseidonHash}(P, v, t, s, h, u) $$
$$ C' = \text{ZeroCond}(v, C) $$
$$ R = \text{MerkleRoot}(l, p, C') $$
$$ U = \text{PoseidonHash}(u, u_{\text{blind}}) $$
$$ Z = zK $$

$G$ and $H$ are the same generators used in `Mint_V1`, $K$ is the
generator in the codebase known as `NULLIFIER_K`:

* `src/sdk/src/crypto/constants/fixed_bases/nullifier_k.rs`

`ZeroCond` is a conditional selection: `f(a, b) = if a == 0 {a} else {b}`.
We use this because the Merkle tree is instantiated with a fake coin of
value 0 and so we're able to produce dummy inputs of value 0.

### Contract call creation

Assuming a coin $C$ exists on the blockchain on leaf position $l$ and
does not have a corresponding published nullifier $N$, it can be spent.
To create the necessary proofs, Alice uses the known values of her
coin $C$ and picks other values that are needed to create a new coin
$C'$ that will be minted to Bob after $C$ is spent.

**Values for `Burn_V1`:**

1. Alice picks a random element $z$ from $F_p$ to use as the secret key
   in order to sign the transaction.
2. Alice picks a random element $v_{\text{blind}}$ from $F_q$ to use
   as the blinding factor for $V$.
3. Alice picks a random element $t_{\text{blind}}$ from $F_p$ to use
   as the blinding factor for $T$.
4. Alice creates the `Burn_V1` ZK proof using the existing known values
   of her coin $C$ and the values picked above.

**Values for `Mint_V1`:**

1. Alice picks a random element $s$ from $F_p$ to use as a unique serial
   number for the new coin $C'$.
2. Alice optionally chooses a contract ID to use as $h$ or uses `ZERO`
   if $h$ does not have to call another contract.
3. Alice optionally chooses necessary data for $u$ or uses `ZERO`
   if no data has to be passed.
4. Alice chooses the corresponding $v_{\text{blind}}$ to be able to
   enforce the Pedersen commitment correctness ($\infty + V - V'$ has
   to evaluate to $\infty$)
5. Alice creates the `Mint_V1` ZK proof using the existing known values
   and the values picked above.

After creating the proofs, Alice builds a transaction containing a
number of inputs that were created with `Burn_V1` and a number of
outputs created with `Mint_V1`.

```rust
{{#include ../../../../../src/contract/money/src/model/mod.rs:money-params}}
```

This gets encoded into the `Transaction` format and the transaction is
signed with a Schnorr signature scheme using the $z$ secret key chosen
in `Burn_V1`.

### Contract call execution

For `MoneyFunction::TransferV1`, we have the following functions, in
order:

* [`money_transfer_get_metadata_v1`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/money/src/entrypoint/transfer_v1.rs#L42)
* [`money_transfer_process_instruction_v1`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/money/src/entrypoint/transfer_v1.rs#L106)
* [`money_transfer_process_update_v1`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/money/src/entrypoint/transfer_v1.rs#L258)
