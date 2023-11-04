# Proposal

EC field element [commitment](../crypto/commitment.md) to: $(proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, proposal^{tokenId}, bulla)$ with blinding factor $blind^{proposal}$

| Proposal                   | Destination                                   |
|----------------------------|-----------------------------------------------|
| $proposal^{destination}_x$ | proposal destination [public key](../crypto/keypair.md) x coordinate  |
| $proposal^{destination}_y$ | proposal destination [public key](../crypto/keypair.md) y coordinate  |
| $proposal^{amount}$        | proposal amount in proposal token             |
| $proposal^{tokenId}$       | proposal [token id](../payment/token_id.md)                             |
| bulla                      | dao [bulla](bulla.md)                                     |
| $blind^{proposal}$         | proposal [commitment](../crypto/commitment.md) blind factor              |
