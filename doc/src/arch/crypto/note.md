# note
coin is stream cipher encrypted with symmetric key `key` derived from shared key[^1].
`key` = hash(sharedSecret, ephemeralKey)
$sharedSecret = ephemeralSecret \mul publicKey$ where `publicKey` is recipient public key. based off diffie-hellman shared secret.

## payment note
Note = (sn, value, tokenId, spendHook, data, blind^{value}, blind^{token}, memo)

| note            | description                    |
|-----------------|--------------------------------|
| sn              | serial number sampled at random|
| value           | payment value                  |
| tokenId         | token id                       |
| spendHook       | coin related contract          |
| data            | data used by the coin contract |
| $blind^{value}$ | value commitment blinding term |
| $blind^{token}$ | token commitment blinding term |
| memo            | arbitrary data                 |
