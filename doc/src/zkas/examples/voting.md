Anonymous voting
================

Anonymous voting[^1] is a type of voting process where users can
vote without revealing their identity, by proving they are accepted
as valid voters.

The proof enables user privacy and allows for fully anonymous voting.

The starting point is a Merkle proof[^2], which efficiently proves that
a voter's key belongs to a Merkle tree. However, using this proof alone
would allow the organizer of a process to correlate each vote envelope
with its voter's key on the database, so votes wouldn't be secret.

## Vote proof

```zkas
{{#include ../../../../proof/voting.zk}}
```

Our proof consists of four main operation. First we are hashing the
_nullifier_ using our secret key and the hashed process ID. Next,
we derive our public key and hash it. Following, we take this hash
and create a Merkle proof that it is indeed contained in the given
Merkle tree. And finally, we create a _Pedersen commitment_[^3]
for the vote choice itself.

Our vector of public inputs can look like this:

```
let public_inputs = vec![
    nullifier,
    merkle_root,
    *vote_coords.x(),
    *vote_coords.y(),
]
```

And then the Verifier uses these public inputs to verify the given
zero-knowledge proof.

[^1]: Specification taken from [vocdoni franchise proof](https://docs.vocdoni.io/architecture/protocol/anonymous-voting/zk-census-proof.html)

[^2]: [Merkle tree on Wikipedia](https://en.wikipedia.org/wiki/Merkle_tree)

[^3]: See section 3: _The Commitment Scheme_ of Torben Pryds Pedersen's
    [paper on Non-Interactive and
    Information-Theoretic Secure Verifiable Secret
    Sharing](https://link.springer.com/content/pdf/10.1007%2F3-540-46766-1_9.pdf)
