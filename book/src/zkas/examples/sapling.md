# Sapling payment scheme

Generally, the Sapling payment scheme consists of two proofs -
**mint** and **burn**. We use the mint proof to create a new _coin_
$C$, and we use the burn proof to spend a previously minted _coin_.

## Mint

```
{{#include ../../../../zkas/proofs/mint.zk}}
```

As you can see, the `Mint` contract/circuit basically consists of
three operations. First one is hashing the _coin_ $C$, and after
that, we create _Pedersen commitments_ for both the coin's **value**
and the coin's **token ID**. On top of the zkas code, we've declared
two constant values that we are going to use for multiplication in
the commitments.

The `constrain_instance` call can take any of our assigned variables
and enforce a _public input_. Public inputs are an array (or vector)
of values used as public inputs by verifiers to verify a zero knowledge
proof. In the above case of the Mint contract, since we have five
calls to `constrain_instance`, we would also have an array of five
elements that represent these public inputs. The array's order **must**
match the `constrain_instance` calls since they will be constrained
by their index in the array (which is incremented for every call).

In other words, the vector of public inputs could look like this:

```rust
let public_inputs = vec![
    coin,
    *value_commitment_coords.x(),
    *value_commitment_coords.y(),
    *token_commitment_coords.x(),
    *token_commitment_coords.y(),
];
```

And then the Verifier uses these public inputs to verify a given zero 
knowledge proof.

### Coin

During the **Mint** phase we create a new coin $C$, which is bound
to the public key $P$. The coin $C$ is publicly revealed on the
blockchain and added to the Merkle tree.

Let $v$ be the coin's value, $t$ be the token ID, $\rho$ be the unique
serial number for the coin, and $r_C$ be a random blinding value. We
create a commitment (hash) these elements and produce the coin $C$ in
zero-knowledge:

$$C = H(P, v, t, \rho, r_C)$$

### Value and Token commitments

To have some value $v$ for our coin, we can create a
_Pedersen commitment_ $V$ where $r_V$ is the blinding factor for the
commitment, and $G_1$ and $G_2$ are two predefined generators:

$$ v > 0 $$
$$ V = vG_1 + r_VG_2 $$

The token ID can be thought of as an attribute we append to our _coin_
so we can have a differentiation of assets we are working with. In
practice for example, this allows us to work with different tokens,
using the same zero-knowledge proof circuit. For this token ID, we can
also build a _Pedersen commitment_ $T$, where $t$ is the token ID,
$r_T$ is the blinding factor, and $G_1$ and $G_2$ are the generators:

$$ T = tG_1 + r_TG_2 $$

Knowing this, we can extend our code example and build the
before-mentioned public inputs for the circuit:

```rust
let public_key = pallas::Point::random(&mut OsRng);
let coords = public_key.to_affine().coordinates().unwrap();
let pub_x = *coords.x();
let pub_y = *coords.y();

let value = pallas::Base::from(42);
let token = pallas::Base::from(1);
let serial = pallas::Base::random(&mut OsRng);
let coin_blind = pallas::Base::random(&mut OsRng);

let coin = poseidon::Hash(pub_x, pub_y, value, token, serial, coin_blind);

let value_commit = pedersen_commitment_u64(value, value_blind);
let value_coords = value_commit.to_affine().coordinates().unwrap();

let token_commit = pedersen_commitment_u64(token, token_blind);
let token_coords = token_commit.to_affine().coordinates().unwrap();

let public_inputs = vec![
    coin,
    *value_commitment_coords.x(),
    *value_commitment_coords.y(),
    *token_commitment_coords.x(),
    *token_commitment_coords.y(),
];
```

## Burn

```
{{#include ../../../../zkas/proofs/burn.zk}}
```

The `Burn` contract/circuit consists of operations similar to the
`Mint` circuit, with the addition of _Merkle root_ calculation. In
the same manner, we're doing a Poseidon hash instance, we're building
Pedersen commitments for the value and token ID, and finally we're
doing a public key derivation.

In this case, our vector of public inputs could look like:

```rust
let public_inputs = vec![
    nullifier,
    *value_commitment_coords.x(),
    *value_commitment_coords.y(),
    *token_commitment_coords.x(),
    *token_commitment_coords.y(),
    merkle_root,
    *signature_coords.x(),
    *signature_coords.y(),
];
```

### Nullifier

When we spend the coin, we must ensure that the value of the coin
cannot be double spent. We call this the _Burn_ phase. The process
relies on a nullifier $N$, which we create using the secret key $x$
for the public key $P$ and a unique random serial $\rho$. Nullifiers
are unique per coin and prevent double spending:

$$ N = H(x, \rho) $$


### Value and Token commitments

Just like we calculated these for the `Mint` contract, we do the same
here:

$$ v > 0 $$
$$ V = vG_1 + r_VG_2 $$
$$ T = tG_1 + r_TG_2 $$


### Public key derivation

We check that the secret key $x$ corresponds to a public key $P$.
Usually, we do public key derivation by multiplying our secret key
with a generator $G$, which results in a public key.

$$ P = xG $$

### Merkle root

We check that the merkle root corresponds to a coin which is in the
Merkle tree $R$:

$$ C = H(P, v, t, \rho, r_C) $$
$$ C \in R $$

Knowing this, we can extend our code example and build the
before-mentioned public inputs for the circuit:

```rust
let secret_key = pallas::Base::random(&mut OsRng);
let serial = pallas::Base::random(&mut OsRng);

let nullifier = poseidon::Hash(secret_key, serial);

let value = 42;
let token = 1;
let value_blind = pallas::Scalar::random(&mut OsRng);
let token_blind = pallas::Scalar::random(&mut OsRng);

let value_commit = pedersen_commitment_u64(value, valie_blind);
let value_coords = value_commit.to_affine().coordinates().unwrap();

let token_commit = pedersen_commitment_u64(token, token_blind);
let token_coords = token_commit.to_affine().coordinates().unwrap();

let tree = BridgeTree::<MerkleNode, 32>::new(100);
let some_coin_0 = pallas::Base::random(&mut OsRng);
let some_coin_1 = pallas::Base::random(&mut OsRng);
tree.append(some_coin_0);
tree.witness();
tree.append(some_coin_1);
tree.witness();

let merkle_root = tree.root();

let sig_secret = pallas::Base::random(&mut OsRng);
let sig_public = OrchardFixedBases::NullifierK.generator() * mod_r_p(sig_secret);
let sig_coords = sig_public.to_affine().coordinates().unwrap();

let public_inputs = vec![
    nullifier,
    *value_commit_coords.x(),
    *value_commit_coords.y(),
    *token_commit_coords.x(),
    *token_commit_coords.y(),
    merkle_root,
    *sig_coords.x(),
    *sig_coords.y(),
];
```
