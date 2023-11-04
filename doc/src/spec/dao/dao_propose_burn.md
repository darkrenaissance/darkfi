# Dao propose burn

$$ X = (nullifier, cm^{value}_x, cm^{value}_y, cm^{token}, root,  signature^{public}_x, signature^{public}_y)$$

$$ W = (sk, sn, spendHook, data, value, tokenId, blind^{value}, blind^{token}, pos, path, signature^{secret}) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Burn dao proposal
- Derive, and reveal [nullifier](../crypto/nullifier.md)
- Calculate, and reveal value [commitment](../crypto/commitment.md)
- Calculate, and reveal [token](../payment/token_id.md) [commitment](../crypto/commitment.md)
- Add input [coin](../payment/coin.md) to [merkle tree](../crypto/merkletree.md), and reveal it's root.
- Reveal associated spendHook contract.
- Derive, and reveal [signature](../crypto/signature.md) public key [$signature^{public}$](../crypto/keypair.md)


| Public Input         | Description                                                                                 |
|----------------------|---------------------------------------------------------------------------------------------|
| nullifier            | dao's proposal [coin](../payment/coin.md) [nullifier](../crypto/nullifier.md)                                             |
| $cm^{value}_x$       | x coordinate of value point [commitment](../crypto/commitment.md)                           |
| $cm^{value}_y$       | y coordinate of value point [commitment](../crypto/commitment.md)                           |
| $cm^{token}$         | [commitment](../crypto/commitment.md) of [tokenId](../payment/token_id.md) as field element |
| root                 | root of commitments [merkle tree](../crypto/merkletree.md) of [coin](../payment/coin.md)s   |
|$signature^{public}_x$| [signature](../crypto/signature.md) [public key](../crypto/keypair.md) x coordinate         |
|$signature^{public}_y$| [signature](../crypto/signature.md) [public key](../crypto/keypair.md) y coordinate         |


| Witnesses            | Description                                          |
|----------------------|------------------------------------------------------|
| sk                   | [proposal](proposal.md) [coin](../payment/coin.md) [secret key](../crypto/keypair.md)     |
| sn                   | [proposal](proposal.md) [coin](../payment/coin.md) serial number                          |
| spendHook            | burn spendHook contract                                |
| data                 | spendHook contract input data                        |
| value                | [proposal](proposal.md) [coin](../payment/coin.md) value                                  |
| tokenId              | [proposal](proposal.md) [coin](../payment/coin.md) [token id](../payment/token_id.md)                                    |
| $blind^{value}$      | [proposal](proposal.md) value [commitment](../crypto/commitment.md) blinding term              |
| $blind^{token}$      | [token](../payment/token_id.md) [commitment](../crypto/commitment.md) blinding term                       |
| pos                  | [proposal](proposal.md) [coin](../payment/coin.md) leaf position in [merkle tree](../crypto/merkletree.md)           |
| path                 | [proposal](proposal.md) [coin](../payment/coin.md) path in [merkle tree](../crypto/merkletree.md)                    |
| $signature^{secret}$ | [proposal](proposal.md) [signature](../crypto/signature.md) [secret key](../crypto/keypair.md)                            |
