# Coin

Field element [commitment](../crypto/commitment.md) to $(pub_x, pub_y, value, token, sn, spendHook, data)$

| Coin inputs          | Description                                       |
|----------------------|---------------------------------------------------|
| $pub_x$              | [public key](../crypto/keypair.md) x coordinate                           |
| $pub_y$              | [public key](../crypto/keypair.md) y coordinate                           |
| value                | coin value                                        |
| token                | coin [token id](token_id.md)                                     |
| sn                   | coin serial number                                |
| spendHook            | contract to be executed upon minting that coin    |
| data                 | data required by spendHook                        |
