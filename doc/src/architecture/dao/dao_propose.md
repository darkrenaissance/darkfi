# dao propose
$$ X = (cm^{token}, root^{bulla}, proposal, value^{total}_x, value^{total}_y) $$
$$ W = (value^{total}, blind^{value^{total}}, blind^{token}, proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, proposal^{tokenId}, blind^{proposal}, proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, pub_x, pub_y, blind^{bulla}, pos, path) $$
$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| public input               |                                               |
|----------------------------|-----------------------------------------------|
| $cm^{token}$               | proposal token commitment as field element    |
| $root^{bulla}$             | root of bulla in merkle tree                  |
| proposal                   | dao proposer proposal                         |
| $value^{total}_x$          | total fund commitment's x coordinate          |
| $value^{total}_y$          | total fund commitment's y coordinate          |

| witnesses                | Description                                            |
|--------------------------|--------------------------------------------------------|
| $value^{total}$          | total proposal funds value                             |
| $blind^{value^{total}}$  | blinding value for $value^{total}$ commitment           |
| $blind^{token}$          | proposal token commitment blinding factor              |
|$proposal^{destination}_x$| proposal destination public key x coordinate           |
|$proposal^{destination}_y$| proposal destination public key y coordinate           |
| $proposal^{amount}$      | proposal amount in proposal token                      |
| $proposal^{tokenId}$     | proposal token id                                      |
| $blind^{proposal}$       | proposal commitment blinding term                      |
| proposerLimit            | governance token necessary for the vote to be valid    |
| quorum                   | minimum number of votes necessary to pass the proposal |
| $approvalRatio_{quot}$   | proposal approval ratio quotient                       |
| $approvalRatio_{base}$   | proposal approval ratio base                           |
| tokenId                  | governance token id                                    |
| $pub_x$                  | proposal public key x coordinate                       |
| $pub_y$                  | proposal public key y coordinate                       |
| $blind^{bulla}$          | bulla commitment blinding factor                       |
| pos                      | bulla leaf position in the merkle tree                 |
| path                     | path of the bulla leaf at position `pos`
      |

# circuit checks

- $proposal^{amount} > 0 $
- ${proposerLimit <= value^{total} $
