# Dao propose

$$ X = (cm^{token}, root^{bulla}, proposal, cm^{value^{total}}_x, cm^{value^{total}}_y) $$

$$ W = (value^{total}, blind^{value^{total}}, blind^{token}, proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, proposal^{tokenId}, blind^{proposal}, proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, pub_x, pub_y, blind^{bulla}, pos, path) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Create dao proposal

- Calculate, and reveal [token](../payment/tokne_id.md) [commitment](../crypto/commitment.md)
- Calculate, and reveal [bulla](bulla.md)
- Calculate, and reveal [proposal](proposal.md)
- Calculate, and reveal total proposers funds [commitment](../crypto/commitment.md)

| Public input               | Description                                  |
|----------------------------|-----------------------------------------------|
| $cm^{token}$               | [proposal](proposal.md) [token](../payment/token_id.md) [commitment](../crypto/commitment.md) as field element    |
| $root^{bulla}$             | root of [bulla](bulla.md) in [merkle tree](../crypto/merkletree.md)                  |
| proposal                   | dao proposer [proposal](proposal.md)                         |
| $cm^{value^{total}}_x$          | total funds [commitment](../crypto/commitment.md)'s x coordinate          |
| $cm^{value^{total}}_y$          | total funds [commitment](../crypto/commitment.md)'s y coordinate          |

| Witnesses                | Description                                               |
|--------------------------|-----------------------------------------------------------|
| $value^{total}$          | total [proposal](proposal.md) funds value                 |
| $blind^{value^{total}}$  | blinding value for $value^{total}$ [commitment](../crypto/commitment.md)  |
| $blind^{token}$          | token [commitment](../crypto/commitment.md) blinding factor  |
|$proposal^{destination}_x$| destination [public key](../crypto/keypair.md) x coordinate           |
|$proposal^{destination}_y$| destination [public key](../crypto/keypair.md) y coordinate           |
| $proposal^{amount}$      | amount in [proposal](proposal.md) token                      |
| $proposal^{tokenId}$     | [proposal](proposal.md) [token id](../payment/token_id.md)   |
| $blind^{proposal}$       | [proposal](proposal.md) [commitment](../crypto/commitment.md) blinding term             |
| proposerLimit            | governance token necessary for the vote to be valid    |
| quorum                   | minimum number of votes necessary to pass the [proposal](proposal.md) |
| $approvalRatio_{quot}$   | [proposal](proposal.md) approval ratio quotient                       |
| $approvalRatio_{base}$   | [proposal](proposal.md) approval ratio base                           |
| tokenId                  | governance [token id](../payment/token_id.md)                                    |
| $pub_x$                  | [proposal](proposal.md) [public key](../crypto/keypair.md) x coordinate                       |
| $pub_y$                  | [proposal](proposal.md) [public key](../crypto/keypair.md) y coordinate                       |
| $blind^{bulla}$          | [bulla](bulla.md) [commitment](../crypto/commitment.md) blinding factor                       |
| pos                      | [bulla](bulla.md) leaf position in the [merkle tree](../crypto/merkletree.md)                 |
| path                     | path of the [bulla](bulla.md) leaf at pos |

# Circuit checks

- $proposal^{amount} > 0$
- $proposerLimit <= value^{total}$
