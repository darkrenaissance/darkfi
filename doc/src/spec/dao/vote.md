# Vote

Vote on [proposal](proposal.md) by yes, or no by $proposal^{amount}$ of dao tokens.

$$ X = (cm^{token}, proposal, cm^{vote^{yes}}_x, cm^{vote^{yes}}_y, cm^{vote^{all}}_x, cm^{vote^{all}}_y) $$

$$ W = (proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, tokenId, blind^{proposal}, proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, pub_x, pub_y, blind^{bulla}, vote^{yes}, blind^{vote^{yes}}, vote^{yes}, vote^{all}, blind^{vote^{all}}, blind^{token}) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Dao vote proof

- Calculate, and reveal [proposal](proposal.md) [token](../payment/token_id.md) [commitment](../crypto/commitment.md) to vote on
- Derive, and reveal [proposal](proposal.md)
- Calculate, and  Reveal yes-vote 0/1 for no/yes [commitment](../crypto/commitment.md) $=vote^{yes}*vote^{all^}$
- Reveal all-vote value [commitment](../crypto/commitment.md)


| Public inputs       | Description                                |
|---------------------|--------------------------------------------|
| $cm^{token}$        | [proposal](proposal.md) token [commitment](../crypto/commitment.md) as field element |
| proposal            | [proposal](proposal.md) [commitment](../crypto/commitment.md) as field element       |
| $cm^{vote^{yes}}_x$ |  yes vote [commitment](../crypto/commitment.md) x coordinate            |
| $cm^{vote^{yes}}_y$ | yes vote [commitment](../crypto/commitment.md) y coordinate           |
| $cm^{vote^{all}}_x$ | all votes [commitment](../crypto/commitment.md) x coordinate          |
| $cm^{vote^{all}}_y$ | all votes [commitment](../crypto/commitment.md) y coordinate          |


| Witnesses                  | Description                                            |
|----------------------------|--------------------------------------------------------|
| $proposal^{destination}_x$ | [proposal](proposal.md) destination [public key](../crypto/keypair.md) x coordinate           |
| $proposal^{destination}_y$ | [proposal](proposal.md) destination [public key](../crypto/keypair.md) y coordinate           |
| $proposal^{amount}$        |  amount in [proposal](proposal.md) token                      |
| tokenId                    | [proposal](proposal.md) token id                                      |
| $blind^{proposal}$         | [proposal](proposal.md) [commitment](../crypto/commitment.md) blinding factor                    |
| proposerLimit              | governance token necessary for the vote to be valid    |
| quorum                     | minimum number of votes necessary to pass the [proposal](proposal.md) |
| $approvalRatio_{quot}$     | [proposal](proposal.md) approval ratio quotient                       |
| $approvalRatio_{base}$     | [proposal](proposal.md) approval ratio base                           |
| tokenId                    | governance [token id](../payment/token_id.md)                                    |
| $pub_x$                    | dao [public key](../crypto/keypair.md) x coordinate                            |
| $pub_y$                    | dao [public key](../crypto/keypair.md) y coordinate                            |
| $blind^{bulla}$            | [bulla](bulla.md) [commitment](../crypto/commitment.md) blinding factor                       |
| $vote^{yes}$               | yes vote direction a boolean as either 0/1 for no/yes  |
| $blind^{vote^{yes}}$       | yes vote [commitment](../crypto/commitment.md) blinding factor                    |
| $vote^{all}$               | all votes value                                       |
| $blind^{vote^{all}}$       | blinding term for all votes [commitment](../crypto/commitment.md)s                |
| $blind^{token}$            | governance token blinding term                        |

# Circuit checks

- Validate that $vote^{yes}$ is either 0, or 1.
