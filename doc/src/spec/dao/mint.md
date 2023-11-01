# Mint contract

Mint a new dao [bulla](bulla.md).

$$ X = (pub_x, pub_y, bulla) $$

$$ W = (proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, sk, blind^{bulla}) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Mint bulla

- Derive, and reveal dao authority [public key](../crypto/keypair.md).
- Calculate, and reveal [bulla](bulla.md).

| Public input | Description                          |
|--------------|--------------------------------------|
| $pub_x$      | dao [public key](../crypto/keypair.md) EC point x coordinate |
| $pub_y$      | dao [public key](../crypto/keypair.md) EC point y coordinate |
| bulla        | [bulla](bulla.md) field element [commitment](../crypto/commitment.md)       |

| Witnesses              | Description                                            |
|------------------------|--------------------------------------------------------|
| proposerLimit          | governance token necessary for the vote to be valid    |
| quorum                 | minimum number of votes necessary to pass the [proposal](proposal.md) |
| $approvalRatio_{quot}$ | [proposal](proposal.md) approval ratio quotient                       |
| $approvalRatio_{base}$ | [proposal](proposal.md) approval ratio base                           |
| tokenId                | governance [token id](../payment/token_id.md)                                    |
| sk                     | dao [secret key](../crypto/keypair.md)                                         |
| $blind^{bulla}$        | [bulla](bulla.md) [commitment](../crypto/commitment.md) blinding factor                       |
