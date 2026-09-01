# Scheme

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $G, H$ be the Pedersen commitment generators `VALUE_COMMIT_VALUE`
(`src/sdk/src/crypto/constants/fixed_bases/value_commit_v.rs`) and
`VALUE_COMMIT_RANDOM` (`src/sdk/src/crypto/constants/fixed_bases/value_commit_r.rs`),
and let $K$ be the `NULLIFIER_K` generator
(`src/sdk/src/crypto/constants/fixed_bases/nullifier_k.rs`).

## Transfer

This function transfers value by burning a set of coins $𝐂$, and minting a
set of coins, such that the value spent and created are equal. Value
conservation is enforced per token: for every token commitment, the sum of
the input Pedersen value commitments must equal the sum of the output ones.

* Wallet:
  * Builder: `src/contract/money/src/client/transfer_v1/builder.rs`
  * Convenience methods: `src/contract/money/src/client/transfer_v1/mod.rs`
  * Build proofs: `src/contract/money/src/client/transfer_v1/proof.rs`
* WASM VM code: `src/contract/money/src/entrypoint/transfer_v1.rs`
* ZK proofs:
  * `src/contract/money/proof/burn_v1.zk`
  * `src/contract/money/proof/mint_v1.zk`

### Function Params

Let $\t{MoneyInput}, \t{MoneyOutput}$ be defined as in [Inputs and Outputs](model.md#inputs-and-outputs).

Define the Money transfer function params
$$ \begin{aligned}
  𝐢 &∈ \t{MoneyInput}^* \\
  𝐨 &∈ \t{MoneyOutput}^*
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model/mod.rs:money-params}}
```

### Contract Statement

Let $π_\t{mint}, π_\t{burn}$ be defined as in [ZK Proofs](#zk-proofs).

Each input $i ∈ 𝐢$ carries a `Burn_V1` proof, and each output $o ∈ 𝐨$
carries a `Mint_V1` proof. Additionally the contract verifies:

* **Valid coin merkle root** &emsp; $i.R$ is a previously seen merkle root
  in the money contract merkle roots DB (or in the transaction-local one
  if $i.\t{tx\_local}$ is set).
* **Unused nullifier** &emsp; $i.N$ does not exist in the money contract
  nullifiers set, and is unique within the call.
* **Unique coins** &emsp; each output coin $o.C$ has not been seen before,
  neither on chain, transaction-locally, nor within the call.
* **Per token value conservation** &emsp; grouping inputs and outputs by
  their token commitment $T$, for every group:
  $$ \sum_{i ∈ 𝐢, i.T = T} i.V = \sum_{o ∈ 𝐨, o.T = T} o.V $$
* **Spend hook enforcement** &emsp; the spend hook $h$ verified in the
  `Burn_V1` proofs is computed as the function ID of the parent call, or
  `FuncId::none()` if the transfer is a root call. This way coins carrying
  a spend hook can only be spent by the contract function they point to.

### ZK Proofs

#### `Mint_V1`

Using the `Mint_V1` circuit, we are able to create outputs
in our UTXO set. It is used along with the `Burn_V1` circuit in
`MoneyFunction::TransferV1` where we perform a payment to some address
on the network.

Denote this proof by $π_\t{mint}$.

**Circuit witnesses:**

* $P_x, P_y$ - Coordinates of the recipient public key which go into the coin commitment (pallas base field elements)
* $v$ - Value of the coin (unsigned 64-bit integer)
* $τ$ - Token ID of the coin (pallas base field element)
* $h$ - Spend hook, allows composing this ZK proof to invoke other contracts (pallas base field element)
* $u$ - Data passed from this coin to the invoked contract (pallas base field element)
* $b$ - Blinding factor of the coin, ensuring its uniqueness (pallas base field element)
* $v_\t{blind}$ - Random blinding factor for a Pedersen commitment to $v$ (pallas scalar field element)
* $τ_\t{blind}$ - Random blinding factor for a commitment to $τ$ (pallas base field element)

**Circuit public inputs:**

* $C$ - Coin commitment
* $V$ - Pedersen commitment to $v$
* $T$ - Token ID commitment

**Circuit:**

$$ C = \text{PoseidonHash}(P_x, P_y, v, τ, h, u, b) $$
$$ V = vG + v_{\text{blind}}H $$
$$ T = \text{PoseidonHash}(τ, τ_{\text{blind}}) $$

The 64-bit range of $v$ is enforced implicitly: $G$ is a *short* fixed
base (`VALUE_COMMIT_VALUE`) and its scalar multiplication (`ec_mul_short`)
only admits witnesses of at most $L_\t{value} = 64$ bits.

#### `Burn_V1`

Using the `Burn_V1` circuit, we are able to create inputs in
our UTXO set. It is used along with the `Mint_V1` circuit in
`MoneyFunction::TransferV1` where we perform a payment to some address
on the network.

Denote this proof by $π_\t{burn}$.

**Circuit witnesses:**

* $x$ - Secret key used to derive the coin's public key $P$ and the nullifier (pallas base field element)
* $v$ - Value of the coin being spent (unsigned 64-bit integer)
* $τ$ - Token ID of the coin being spent (pallas base field element)
* $h$ - Spend hook, allows composing this ZK proof to invoke other contracts (pallas base field element)
* $u$ - Data passed from this coin to the invoked contract (pallas base field element)
* $b$ - Blinding factor of the coin being spent (pallas base field element)
* $v_{\text{blind}}$ - Random blinding factor for a Pedersen commitment to $v$ (pallas scalar field element)
* $τ_{\text{blind}}$ - Random blinding factor for a commitment to $τ$ (pallas base field element)
* $u_{\text{blind}}$ - Blinding factor for encrypting $u$ (pallas base field element)
* $l$ - Leaf position of $C$ in the Merkle tree of all coin commitments (unsigned 32-bit integer)
* $p$ - Merkle path to the coin $C$ in the Merkle tree (array of 32 pallas base field elements)
* $z$ - Secret key used to derive the signature public key $Z$

**Circuit public inputs:**

* $N$ - Published nullifier to prevent double spending
* $V$ - Pedersen commitment to $v$
* $T$ - Token ID commitment
* $R$ - Merkle root calculated from $l$ and $p$
* $U$ - Commitment to $u$
* $h$ - Spend hook
* $Z$ - Public key derived from $z$ used for transaction signing

**Circuit:**

$$ P = xK $$
$$ C = \text{PoseidonHash}(\mathcal{X}(P), \mathcal{Y}(P), v, τ, h, u, b) $$
$$ N = \text{PoseidonHash}(x, C) $$
$$ V = vG + v_{\text{blind}}H $$
$$ T = \text{PoseidonHash}(τ, τ_{\text{blind}}) $$
$$ C' = \text{ZeroCond}(v, C) $$
$$ R = \text{MerkleRoot}(l, p, C') $$
$$ U = \text{PoseidonHash}(u, u_{\text{blind}}) $$
$$ Z = zK $$

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
3. Alice picks a random element $u_{\text{blind}}$ from $F_p$ to use
   as the blinding factor for $U$.
4. Alice creates the `Burn_V1` ZK proof using the existing known values
   of her coin $C$ and the values picked above.

**Values for `Mint_V1`:**

1. Alice picks a random element $b$ from $F_p$ to use as the blinding
   factor for the new coin $C'$, which guarantees its uniqueness.
2. Alice optionally chooses a contract function ID to use as $h$ or uses `ZERO`
   if the coin does not have to call another contract.
3. Alice optionally chooses necessary data for $u$ or uses `ZERO`
   if no data has to be passed.
4. Alice chooses $v_{\text{blind}}$ for the last output such that the
   value blinds of all inputs and outputs cancel each other out. This
   way the Pedersen commitment homomorphism enforces that the spent and
   minted values are equal (see *per token value conservation* above).
5. Alice creates the `Mint_V1` ZK proof using the existing known values
   and the values picked above.

A single token blind $τ_\t{blind}$ is drawn for the whole call and reused
for every token commitment, so that all inputs and outputs of the call
commit to the same token.

After creating the proofs, Alice builds a transaction containing a
number of inputs that were created with `Burn_V1` and a number of
outputs created with `Mint_V1`.

```rust
{{#include ../../../../../src/contract/money/src/model/mod.rs:money-params}}
```

This gets encoded into the `Transaction` format and the transaction is
signed with a Schnorr signature scheme using the $z$ secret keys chosen
when building the `Burn_V1` proofs.

### Contract call execution

For `MoneyFunction::TransferV1`, the `get_metadata`, `process_instruction`
and `process_update` entrypoint phases
(`src/contract/money/src/entrypoint/transfer_v1.rs`) do the following:

* `get_metadata` verifies one `Burn_V1` proof per input against the
  public inputs $(N, V_x, V_y, T, R, U, h, Z_x, Z_y)$ and one `Mint_V1`
  proof per output against $(C, V_x, V_y, T)$, and gathers all the $Z$
  signature public keys.
* `process_instruction` enforces the contract statement described above,
  and produces a state update with the new nullifiers, the new on-chain
  coins, and the new transaction-local coins.
* `process_update` inserts the nullifiers into the nullifier sparse
  Merkle tree, and appends the new coins to the on-chain or
  transaction-local coin Merkle trees.

## Fee

This function attaches a fee to a transaction so it gets included in
a block. Fees are paid in the native token, accumulate per block
height, and are later claimed together with the block reward via
[`Money::PoWRewardV1`](#powreward).

The call data begins with the `u64` fee value, followed by the params.
A single `Fee_V1` proof burns one input coin and mints one output coin
(usually the change) back to the same public key.

* Wallet builder: `src/contract/money/src/client/fee_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/fee_v1.rs`
* ZK proof: `src/contract/money/proof/fee_v1.zk`

### Function Params

$$ \begin{aligned}
  \t{input} &∈ \t{MoneyInput} \\
  \t{output} &∈ \t{MoneyOutput} \\
  f_\t{blind} &∈ 𝔽_q \\
  t_\t{blind} &∈ 𝔽ₚ \\
  \t{fee} &∈ ℕ₆₄ \\
\end{aligned} $$

### Contract Statement

The `Fee_V1` circuit is a fusion of `Burn_V1` and `Mint_V1` for a
single input and output sharing the same public key $P = xK$ and token
$τ$. Besides the constraints of both circuits (with the input's spend
hook additionally constrained to be `ZERO`, since fee coins cannot be
hooked, and no `ZeroCond` applied to the input coin), the contract
verifies:

* **Nonzero fee** &emsp; $\t{fee} ≠ 0$.
* **Native token** &emsp; the token commitments of both input and output
  equal $\t{PoseidonHash}(\t{DARK}, t_\t{blind})$ where $\t{DARK}$ is the
  native token ID.
* **Value conservation** &emsp;
  $V_\t{input} - V_\t{output} = \t{fee} G + f_\t{blind} H$.
* **Fee accumulation** &emsp; the paid fee is added to the accumulator
  for the verifying block height.

The remaining checks (valid merkle root, unused nullifier, unique coin)
are as in [Transfer](#transfer).

### Signatures

A single signature is attached, using $\t{input}.Z$ as the signature
public key.

## GenesisMint

This function mints the initial supply of the native token. It is only
valid when verified against the genesis block (height 0), and reuses
the [`Mint_V1`](#mint_v1) proofs for its outputs.

* Wallet builder: `src/contract/money/src/client/genesis_mint_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/genesis_mint_v1.rs`
* ZK proof: `src/contract/money/proof/mint_v1.zk`

### Function Params

$$ \begin{aligned}
  \t{input} &∈ \t{MoneyClearInput} \\
  𝐨 &∈ \t{MoneyOutput}^* \\
\end{aligned} $$

### Contract Statement

* **Genesis only** &emsp; the verifying block height must be 0.
* **Native token only** &emsp; $\t{input}.T$ must be the native token ID,
  and every output token commitment must equal
  $\t{PoseidonHash}(\t{input}.T, \t{input}.t_\t{blind})$.
* **Unique coins** &emsp; as in [Transfer](#transfer); transaction-local
  outputs are not allowed.
* **Value conservation** &emsp;
  $\sum_{o ∈ 𝐨} o.V = vG + v_\t{blind}H$ for the clear input's value
  and blind.

### Signatures

A single signature is attached, using $\t{input}.Z$ as the signature
public key.

## PoWReward

This function mints the proof-of-work reward for a block, including the
fees accumulated for that height via [`Money::FeeV1`](#fee). It reuses
the [`Mint_V1`](#mint_v1) proof for its output.

* Wallet builder: `src/contract/money/src/client/pow_reward_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/pow_reward_v1.rs`
* ZK proof: `src/contract/money/proof/mint_v1.zk`

### Function Params

$$ \begin{aligned}
  \t{input} &∈ \t{MoneyClearInput} \\
  \t{output} &∈ \t{MoneyOutput} \\
\end{aligned} $$

### Contract Statement

* **Next block only** &emsp; the call must be verified against exactly
  the height following the current top block, and not against genesis.
* **Native token only** &emsp; as in [GenesisMint](#genesismint).
* **Correct reward value** &emsp; $\t{input}.v$ must equal the expected
  reward for the block height plus the fees accumulated for that height
  in the fees DB.
* **Unique coin** &emsp; as in [Transfer](#transfer); transaction-local
  outputs are not allowed.

### Signatures

A single signature is attached, using $\t{input}.Z$ as the signature
public key.

## TokenMint

This function mints arbitrary tokens and coins for them, authorizing
the mint via a *child* auth module call. The token ID is derived from
the token attributes (see [Token](model.md#token)), where the parent
authority is the function ID of the attached auth call.

* Wallet builder: `src/contract/money/src/client/token_mint_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/token_mint_v1.rs`
* ZK proof: `src/contract/money/proof/token_mint_v1.zk`

### Function Params

$$ \begin{aligned}
  C &∈ 𝔽ₚ \\
  \t{note} &∈ \t{AeadEncNote} \\
\end{aligned} $$

### Contract Statement

The call must have exactly one child call, acting as the auth module
(e.g. [`Money::AuthTokenMintV1`](#authtokenmint)). Let $h_\t{auth}$ be
the function ID of this child call. The `TokenMint_V1` proof enforces:

* **Token ID integrity** &emsp; $τ = \t{PoseidonHash}(h_\t{auth}, \t{UD}_τ, b_τ)$
  for the token attributes.
* **Coin integrity** &emsp; $C = \t{Coin}(\t{PK}, v, τ, \t{SH}, \t{UD}, b)$
  using the derived token ID $τ$.

Additionally the contract verifies the minted coin $C$ is unique, and
the encrypted note $\t{note}$ carries the coin secrets to the receiver.

### Signatures

No signatures are attached. Authorization is delegated to the child
auth module call.

## AuthTokenMint

This is an auth module for [`Money::TokenMintV1`](#tokenmint) which
authorizes token mints with a Schnorr signature from the token's mint
authority key. Token IDs are bound to their authority by deriving the
token attributes' user data from the authority's public key:
$\t{UD}_τ = \t{PoseidonHash}(\mathcal{X}(\t{PK}_\t{mint}), \mathcal{Y}(\t{PK}_\t{mint}))$.

* Wallet builder: `src/contract/money/src/client/auth_token_mint_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/auth_token_mint_v1.rs`
* ZK proof: `src/contract/money/proof/auth_token_mint_v1.zk`

### Function Params

$$ \begin{aligned}
  τ &∈ 𝔽ₚ \\
  \t{PK}_\t{mint} &∈ ℙₚ \\
\end{aligned} $$

### Contract Statement

The parent call must be `Money::TokenMintV1`. Let $h$ be the function
ID of this function itself. The `AuthTokenMint_V1` proof enforces:

* **Mint authority key ownership** &emsp; $\t{PK}_\t{mint} = zK$.
* **Token ID integrity** &emsp; $τ = \t{PoseidonHash}(h, \t{PoseidonHash}(\mathcal{X}(\t{PK}_\t{mint}), \mathcal{Y}(\t{PK}_\t{mint})), b_τ)$.
* **Coin integrity** &emsp; the minted coin $C$ (taken from the parent
  `Money::TokenMintV1` call) commits to the derived token ID $τ$.

Additionally the contract verifies the token mint is not frozen in the
token freezes DB.

### Signatures

A single signature is attached, using $\t{PK}_\t{mint}$ as the signature
public key.

## AuthTokenFreeze

This function permanently freezes a token so it can no longer be minted
via [`Money::AuthTokenMintV1`](#authtokenmint). It is authorized by the
same mint authority key the token ID is bound to.

* Wallet builder: `src/contract/money/src/client/auth_token_freeze_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/auth_token_freeze_v1.rs`
* ZK proof: `src/contract/money/proof/auth_token_freeze_v1.zk`

### Function Params

$$ \begin{aligned}
  \t{PK}_\t{mint} &∈ ℙₚ \\
  τ &∈ 𝔽ₚ \\
\end{aligned} $$

### Contract Statement

Let $h$ be the function ID of `Money::AuthTokenMintV1`. The
`AuthTokenFreeze_V1` proof enforces the same **mint authority key
ownership** and **token ID integrity** relations as
[`AuthTokenMint`](#authtokenmint). Additionally the contract verifies
the token is not already frozen, and the state update freezes it in the
token freezes DB.

### Signatures

A single signature is attached, using $\t{PK}_\t{mint}$ as the signature
public key.

## Burn

This function burns (destroys) coins, permanently removing value from
circulation. The call only has inputs and no outputs; the value
committed in the inputs is destroyed. It reuses the
[`Burn_V1`](#burn_v1) proofs.

* Wallet builder: `src/contract/money/src/client/burn_v1.rs`
* WASM VM code: `src/contract/money/src/entrypoint/burn_v1.rs`
* ZK proof: `src/contract/money/proof/burn_v1.zk`

### Function Params

$$ 𝐢 ∈ \t{MoneyInput}^* $$

### Contract Statement

Identical to the input part of [Transfer](#transfer): valid coin merkle
roots, unused nullifiers, and spend hook enforcement via the parent
call's function ID. Since there are no outputs, no value conservation
check is performed — the burned value is simply gone.

### Signatures

For each $i ∈ 𝐢$, a signature corresponding to the public key $i.Z$
is attached.
