# Rate-limit Nullifiers

For an application, each user maintains:

* User registration
* User interactions
* User removal

Points to note:

1. Network cannot reconstruct your key unless you send 2 messages
   within 1 epoch, which well behaving clients don't do.
2. Requires clocks between nodes to be in sync.
3. You create an keypair which is added to the set. These have a
   cost to create which deters spam.

## User registration

* There exists a Merkle tree of published and valid registrations.
* There exists a set of identity commitments in order to maintain
  and avoid duplicates.
* There exists a set of Merkle roots from the above tree which acts
  as a set of registrations that have been rate-limited/banned.

### Registration process

Let $K$ be a constant identity derivation path.

1. Alice generates a secret key $a_0$ and derives an identity
   commitment: $\hash(K, a_0)$
2. Alice publishes the identity commitment.
3. The Network verifies that the identity commitment is not part of the
   set of identity commitments, providing the ability to append it to
   the membership Merkle tree.
4. Alice and the Network append the identity commitment to the set of
   identity commitments, and to the membership Merkle tree.
5. Alice notes down the leaf position in the Merkle tree in order to be
   able to produce valid authentication paths for future interactions.

## User interaction

For each interaction, Alice must create a _ZK_ proof which ensures
the other participants (verifiers) that she is a valid member of the
app and her identity commitment is part of the membership Merkle tree.

The anti-spam rule is also introduced in the protocol. e.g.:

> Users must not make more than N interactions per epoch.

In other words:

> Users must not send more than one message per second.

The anti-spam rule is implemented with a Shamir Secret Sharing
Scheme[^1]. In our case the secret is the user's secret key, and
the shares are parts of the secret key. If Alice sends more than one
message per second, her key can be reconstructed by the Network, and
thus she can be banned. For these claims to hold true, Alice's _ZK_
proof must also include shares of her secret key and the epoch.

### Interaction process

For secret-sharing, we'll use a linear polynomial:

$$ A(x) = a_1 x + a_0 $$

Where:

$$ a_1 = \hash(a_0, N_\text{external}) $$

$$ N_\text{external} = \hash(\text{epoch}, \text{RLN\_ID}) $$

$\text{rln\_identifier}$ is a unique constant per application.

We will also use the internal nullifier
$N_\text{internal}$ as a mechanism to make a
connection between a person and their messages without revealing their
identity:

$$ N_\text{internal} = \hash(a_1, \text{RLN\_ID}) $$

To send a message $M$, we must come up with a share $(x, y)$, given the
above polynomial.

$$ x = \hash(M) $$
$$ y = A(x) $$

We must also use a _zkSNARK_ to prove correctness of the share.

1. Alice wants to send a message `hello`.
2. Alice calculates the field point $(x, y)$.
3. Alice proves correctness using a _zkSNARK_.
4. Alice sends the message and the proof (plus necessary metadata) to
   the Network.
5. The Network verifies membership, the ZK proof, and if the rate-limit
   was reached (by seeing if Alice's secret can be reconstructed).
6. If the key cannot be reconstructed, the message is valid and relayed.
   Otherwise, the Network proceeds with User removal/Slashing.

## User removal

In the case of spam, the secret key can be retrieved from the _SSS_
shares and the Network can use this to add the Merkle root into the
set of slashed users, therefore disabling their ability to send future
messages and requiring them to register with a new key.

### Slashing process

1. Alice sends two messages in the same epoch.
2. The network now has two shares of Alice's secret key:

$$ (x_1, y_1) $$
$$ (x_2, y_2) $$

3. The Network is able to reconstruct the secret key ($k=2$):

$$ a_0 = \sum_{j=0}^{k-1} y_j \prod_{\begin{smallmatrix} m\,=\,0 \\ m\,\ne\,j \end{smallmatrix}}^{k-1} \frac{x_m}{x_m - x_j} $$ 

4. Given $a_0$, a _zkSNARK_ can be produced to add the Merkle root from
   the membership tree to the banned set.

5. Further messages from the given key will not be accepted for as long
   as this root is part of that set.

## Circuits

### Interaction

```zkas
{{#include ../../../script/research/rlnv2/signal.zk}}
```

### Slashing

```zkas
{{#include ../../../script/research/rlnv2/slash.zk}}
```

[^1]: <https://en.wikipedia.org/wiki/Shamir's_Secret_Sharing>
