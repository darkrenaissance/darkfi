# Burn contract

$$ X = (nullifier, cm^{value}_x, cm^{value}_y, cm^{token}, root, data, spendHook, signature^{public}_x, signature^{public}_y) $$

$$ W = (value, token, blind^{value}, blind^{token}, sn, spendHook, data, blind^{data}, sk, pos, path, signature^{secret}) $$

$$ \mathcal{L} = \{X: (W,W) \in \mathcal{R}\} $$

## Burning a coin

- Publish [coin](coin.md)'s [nullifier](../crypto/nullifier.md) to avoid double-spending.
- [Commit](../crypto/commitment.md) to [coin](coin.md)'s value $(cm^{value}_x, cm^{value}_y)$
- [Commit](../crypto/commitment.md) to [tokenId](token_id.md)
- Add [coin](coin.md) to [merkle tree](../crypto/merkletree.md), and set it's root it as instance.
- Set spendHook as instance
- Calculate [$Signature^{public}$](../crypto/signature.md), and set it as instance.


| Public Input         | Description                                                                                           |
|----------------------|-------------------------------------------------------------------------------------------------------|
| nullifier            | coin [nullifier](../crypto/nullifier.md)                                                              |
| $cm^{value}_x$       | x coordinate of value point [commitment](../crypto/commitment.md)                                     |
| $cm^{value}_y$       | y coordinate of value point [commitment](../crypto/commitment.md)                                     |
| $cm^{token}$         | [commitment](../crypto/commitment.md] of [tokenId](token_id.md) as field element                      |
| root                 | root of [coin](coin.md) [commitment](../crypto/commitment.md)s [merkle tree](../crypto/merkletree.md) |
| data                 | data read during execution of burn spendHook contract                                                 |
| spendHook            | burn related contract                                                                                 |
|$signature^{public}_x$| [signature](../crypto/signature.md) public x coordinate                                               |
|$signature^{public}_y$| [signature](../crypto/signature.md) public y coordinate                                               |

| Witnesses            | Description                                                                                                       |
|----------------------|-------------------------------------------------------------------------------------------------------------------|
| value                | burn value                                                                                                        |
| token                | [tokenId](token_id.md)                                                                                            |
| $blind^{value}$      | blinding term for burn value [commitment](../crypto/commitment.md)                                                |
| $blind^{token}$      | blinding term for [tokenId](token_id.md) [commitment](../crypto/commitment.md)                                    |
| sn                   | serial number for burn [coin](coin.md)                                                                            |
| spendHook            | contract related contract                                                                                         |
| data                 | data read during spendHook execution                                                                              |
| $blind^{data}$       | blinding term for data [commitment](../crypto/commitment.md)                                                      |
| sk                   | [coin](coin.md) [private key](../crypto/keypair.md)                                                                                       |
| pos                  | [coin](coin.md) [commitment](../crypto/commitment.md) leaf position in the [merkle tree](../crypto/merkletree.md) |
| path                 | [coin](coin.md) [commitment](../crypto/commitment.md) path in the [merkle tree](../crypto/merkletree.md)          |
| $signature^{secret}$ | [signature](../crypto/signature.md) [secret key](../crypto/keypair.md)                                                                    |

# Circuit checks

- If the [coin](coin.md) has value zero, then [coin](coin.md) is set to zero, with leaf position 0 in the [sparse merkle tree](../crypto/merkletree.md), the aim is prevent burning zero value [coin](coin.md)s.
