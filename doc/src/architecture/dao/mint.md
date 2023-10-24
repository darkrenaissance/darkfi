# Mint contract

$$ X = (pub_x, pub_y, bulla) $$

$$ W = (proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, sk, blind^{bulla}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| public input | Description                          |
|--------------|--------------------------------------|
| $pub_x$      | dao public key EC point x coordinate |
| $pub_y$      | dao public key EC point y coordinate |
| bulla        | bulla field element commitment       |

| witnesses              | Description                                            |
|------------------------|--------------------------------------------------------|
| proposerLimit          | governance token necessary for the vote to be valid    |
| quorum                 | minimum number of votes necessary to pass the proposal |
| $approvalRatio_{quot}$ | proposal approval ratio quotient                       |
| $approvalRatio_{base}$ | proposal approval ratio base                           |
| tokenId                | governance token id                                    |
| sk                     | dao secret key                                         |
| $blind^{bulla}$        | bulla commitment blinding factor                       |
