# Note

note is stream cipher encrypted with symmetric key `key`, derived from shared key[^1].
`key` = hash(sharedSecret, ephemeralKey)
$sharedSecret = ephemeralSecret * publicKey$ where `publicKey` is recipient public key. based off diffie-hellman shared secret.

## Payment Note

Note = (sn, value, tokenId, spendHook, data, $blind^{value}$, $blind^{token}$, memo)

| Note            | Description                    |
|-----------------|--------------------------------|
| sn              | serial number sampled at random|
| value           | payment value                  |
| tokenId         | token id                       |
| spendHook       | coin related contract          |
| data            | data used by the coin contract |
| $blind^{value}$ | value commitment blinding term |
| $blind^{token}$ | token commitment blinding term |
| memo            | arbitrary data                 |
