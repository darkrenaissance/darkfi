# Freeze contract

Burn minted [coin](coin.md)s

$$ X = (authority^{public}_x, authority^{public}_y, token) $$

$$ W = (authority^{secret}) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Freeze token
- Derive mint authority [public key](../crypto/keypair.md) from witness $authority^{secret}$, and set it as instance.
- Calculate, and reveal [tokenId](token_id.md) of the tokens.


| Public Input         | Description                                             |
|----------------------|---------------------------------------------------------|
|$authority^{public}_y$| minting authority [public key](../crypto/keypair.md) y-coordinate               |
|$authority^{public}_x$| minting authority [public key](../crypto/keypair.md) x-coordinate               |
| token                | derived [tokenId](token_id.md)                                        |

| Witnesses            | Description                                         |
|----------------------|-----------------------------------------------------|
| $authority^{secret}$ | minting authority [secret key](../crypto/keypair.md)|
