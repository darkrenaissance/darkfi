# mint contract

$$ X = (cm^{coin}, cm^{value}_x, cm^{value}_y, cm^{token} $$

$$ W = (pk_x, pk_y, value, token, sn, spendHook, data, blind^{value}, blind^{token}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| Public Input         | Description                                             |
|----------------------|---------------------------------------------------------|
| $cm^{coin}$          | coin commitment as field element                        |
| $cm^{value}_x$       | x coordinate of value point commitment                  |
| $cm^{value}_y$       | y coordinate of value point commitment                  |
| $cm^{token}$         | commitment of tokenId as field element                  |

| witnesses            | Description                                         |
|----------------------|-----------------------------------------------------|
| $pk_x$               | coin public key x coordinate                        |
| $pk_y$               | coin public key y coordinate                        |
| value                | burn value                                          |
| token                | token id                                            |
| sn                   | serial number for burn coin                         |
| spendHook            | contract related contract                           |
| data                 | data read during spendHook execution                |
| $blind^{value}$      | blinding term for burn value commitment             |
| $blind^{token}$      | blinding term for token id commitment               |
