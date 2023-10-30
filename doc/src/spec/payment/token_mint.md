# Arbitrary token mint

mint new tokens with arbitrary supply to given recipient.


$$ X = (authority^{public}_x, authority^{public}_y, token, cm^{coin}, cm^{value}_x, cm^{value}_y, cm^{token}) $$

$$ W = (authority^{secret}, value, rcpt_x, rcpt_y, sn, spendHook, data, blind^{value}, blind^{token}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

## New token mint

- Derive, and reveal mint authority [public key](../crypto/keypair.md).
- Derive, and reveal new tokenId.
- Calculate, and reveal new token's [coin](coin.md).
- Calculate, and reveal [coin](coin.md)'s token [commitment](../crypto/commitment.md).


| Public Input         | Description                                                            |
|----------------------|------------------------------------------------------------------------|
|$authority^{public}_y$| minting authority [public key](../crypto/keypair.md) y-coordinate                              |
|$authority^{public}_x$| minting authority [public key](../crypto/keypair.md) x-coordinate                              |
| token                | derived token id                                                       |
| $cm^{coin}$          | [coin](coin.md) [commitment](../crypto/commitment.md) as field element |
| $cm^{value}_x$       | x coordinate of supply point [commitment](../crypto/commitment.md)     |
| $cm^{value}_y$       | y coordinate of supply point [commitment](../crypto/commitment.md)     |
| $cm^{token}$         | [commitment](../crypto/commitment.md) of tokenId as field element      |

| Witnesses            | Description                                                        |
|----------------------|--------------------------------------------------------------------|
| $authority^{secret}$ | minting authority [secret key](../crypto/keypair.md)                                       |
| value                | token minted supply value                                          |
| $rcpt_x$             | token recipient [public key](../crypto/keypair.md) x coordinate                            |
| $rcpt_y$             | token recipient [public key](../crypto/keypair.md) y coordinate                            |
| sn                   | [coin](coin.md) serial number                                      |
| spendHook            | contract related contract                                          |
| data                 | data read during spendHook execution                               |
| $blind^{value}$      | blinding term for burn value [commitment](../crypto/commitment.md) |
| $blind^{token}$      | blinding term for token id [commitment](../crypto/commitment.md)   |
