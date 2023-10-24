# burn contract

$$ X = (nullifier, cm^{value}_x, cm^{value}_y, cm^{token}, root, data, spendHook, signature^{public}_x, signature^{public}_y) $$

$$ W = (value, token, blind^{value}, blind^{token}, sn, spendHook, data, blind^{data}, sk, pos, path, signature^{secret}) $$

$$ \mathcal{L} = \{X:W\in \mathcal{R}\} $$


| Public Input         | Description                                             |
|----------------------|---------------------------------------------------------|
| nullifier            | hash of (sk||sn)                                        |
| $cm^{value}_x$       | x coordinate of value point commitment                  |
| $cm^{value}_y$       | y coordinate of value point commitment                  |
| $cm^{token}$         | commitment of tokenId as field element                  |
| root                 | root of commitments tree                                |
| data                 | data read during execution of burn spendHook contract   |
| spendHook            | burn related contract                                   |
|$signature^{public}_x$| signature public x coordinate                           |
|$signature^{public}_y$| signature public y coordinate                           |


| witnesses            | Description                                         |
|----------------------|-----------------------------------------------------|
| value                | burn value                                          |
| token                | token id                                            |
| $blind^{value}$      | blinding term for burn value commitment             |
| $blind^{token}$      | blinding term for token id commitment               |
| sn                   | serial number for burn coin                         |
| spendHook            | contract related contract                           |
| data                 | data read during spendHook execution                |
| $blind^{data}$       | blinding term for data commitment                   |
| sk                   | coin private key                                    |
| pos                  | coin commitment leaf position in the merkle tree    |
| path                 | coin commitment merkle tree path                    |
| $signature^{secret}$ | signature secret key                                |
