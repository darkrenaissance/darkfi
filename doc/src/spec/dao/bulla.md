# bulla

similar to the payment coin, bulla is EC field element commitment to: (proposerLimit, quorum, $approvalRatio_{quot}$, $approvalRatio_{base}$, tokenId, $pub_x$, $pub_y$) with blinding factor  $blind^{bulla}$

| bulla                  | Description                                            |
|------------------------|--------------------------------------------------------|
| proposerLimit          | governance token necessary for the vote to be valid    |
| quorum                 | minimum number of votes necessary to pass the proposal |
| $approvalRatio_{quot}$ | proposal approval ratio quotient                       |
| $approvalRatio_{base}$ | proposal approval ratio base                           |
| tokenId                | governance token id                                    |
| $pub_x$                | dao public key x coordinate                            |
| $pub_y$                | dao public key y coordinate                            |
| $blind^{bulla}$        | bulla commitment blinding factor                       |
