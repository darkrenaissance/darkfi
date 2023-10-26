# vote

$$ X = (cm^{token}, proposal, cm^{vote^{yes}}_x, cm^{vote^{yes}}_y, cm^{vote^{all}}_x, cm^{vote^{all}}_y) $$

$$ W = (proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, tokenId, blind^{proposal}, proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, pub_x, pub_y, blind^{bulla}, vote^{yes}, blind^{vote^{yes}}, vote^{value}, vote^{all}_{value}, blind^{vote^{all}_{value}}, blind^{token}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| public inputs       | Description                                |
|---------------------|--------------------------------------------|
| $cm^{token}$        | proposal token commitment as field element |
| proposal            | proposal commitment as field element       |
| $cm^{vote^{yes}}_x$ | yes vote commitment x coordinate           |
| $cm^{vote^{yes}}_y$ | yes vote commitment y coordinate           |
| $cm^{vote^{all}}_x$ | all votes commitment x coordinate          |
| $cm^{vote^{all}}_y$ | all votes commitment y coordinate          |

| Witnesses                  | Description                                            |
|----------------------------|--------------------------------------------------------|
| $proposal^{destination}_x$ | proposal destination public key x coordinate           |
| $proposal^{destination}_y$ | proposal destination public key y coordinate           |
| $proposal^{amount}$        | proposal amount in proposal token                      |
| tokenId                    | proposal token id                                      |
| $blind^{proposal}$         | proposal commitment blinding factor                    |
| proposerLimit              | governance token necessary for the vote to be valid    |
| quorum                     | minimum number of votes necessary to pass the proposal |
| $approvalRatio_{quot}$     | proposal approval ratio quotient                       |
| $approvalRatio_{base}$     | proposal approval ratio base                           |
| tokenId                    | governance token id                                    |
| $pub_x$                    | dao public key x coordinate                            |
| $pub_y$                    | dao public key y coordinate                            |
| $blind^{bulla}$            | bulla commitment blinding factor                       |
| $vote^{yes}$               | yes vote a boolean as either 0, or 1                   |
| $blind^{vote^{yes}}$        | yes vote commitment blinding factor                   |
| $vote^{all}_{value}$        | all votes value                                       |
| $blind^{vote^{all}_{value}}$| blinding term for all votes commitments               |
| $blind^{token}$             | governance token blinding term                        |

# circuit checks

- validate that $vote^{yes}$ is either 0, or 1.
