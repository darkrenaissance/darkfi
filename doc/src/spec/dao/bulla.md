# Bulla

Similar to the payment [coin](../payment/coin.md), bulla is EC field element [commitment](../crypto/commitment.md) to: (proposerLimit, quorum, $approvalRatio_{quot}$, $approvalRatio_{base}$, tokenId, $pub_x$, $pub_y$) with blinding factor  $blind^{bulla}$

| Bulla                  | Description                                            |
|------------------------|--------------------------------------------------------|
| proposerLimit          | governance token necessary for the vote to be valid    |
| quorum                 | minimum number of votes necessary to pass the [proposal](proposal.md) |
| $approvalRatio_{quot}$ | [proposal](proposal.md) approval ratio quotient                       |
| $approvalRatio_{base}$ | [proposal](proposal.md) approval ratio base                           |
| tokenId                | governance [token id](../payment/token_id.md)                                    |
| $pub_x$                | dao [public key](../crypto/keypair.md) x coordinate                            |
| $pub_y$                | dao [public key](../crypto/keypair.md) y coordinate                            |
| $blind^{bulla}$        | bulla [commitment](../crypto/commitment.md) blinding factor                       |
