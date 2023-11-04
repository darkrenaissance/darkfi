# Mint contract

$$ X = (cm^{coin}, cm^{value}_x, cm^{value}_y, cm^{token} $$

$$ W = (pk_x, pk_y, value, token, sn, spendHook, data, blind^{value}, blind^{token}) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Minting a coin

- Calculate, and set [coin](coin.md) as instance.
- Calculate [coin](coin.md)'s value [commitment](../crypto/commitment.md), and set it as instance.
- Calculate [coin](coin.md)'s [tokenId](token_id.md) [commitment](../crypto/commitment.md), and set is as instance.


| Public Input         | Description                                                                       |
|----------------------|-----------------------------------------------------------------------------------|
| $cm^{coin}$          | [coin](coin.md) [commitment](../crypto/commitment.md) as field element            |
| $cm^{value}_x$       | x coordinate of value point [commitment](../crypto/commitment.md)                 |
| $cm^{value}_y$       | y coordinate of value point [commitment](../crypto/commitment.md)                 |
| $cm^{token}$         | [commitment](../crypto/commitment.md) of [tokenId](token_id.md) as field element  |

| Witnesses            | Description                                                                    |
|----------------------|--------------------------------------------------------------------------------|
| $pk_x$               | [coin](coin.md) [public key](../crypto/keypair.md) x coordinate                                        |
| $pk_y$               | [coin](coin.md) [public key](../crypto/keypair.md) y coordinate                                        |
| value                | burn value                                                                     |
| token                | [tokenId](token_id.md)                                                         |
| sn                   | [coin](coin.md) serial number                                                  |
| spendHook            | contract related contract                                                      |
| data                 | data read during spendHook execution                                           |
| $blind^{value}$      | blinding term for burn value [commitment](../crypto/commitment.md)             |
| $blind^{token}$      | blinding term for [tokenId](token_id.md) [commitment](../crypto/commitment.md) |
