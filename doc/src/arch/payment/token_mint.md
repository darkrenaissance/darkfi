# arbitrary token mint

mint new tokens with arbitrary supply to given recipient.

# new token mint

$$ X = (authority^{public}_x, authority^{public}_y, token, cm^{coin}, cm^{value}_x, cm^{value}_y), cm^{token} $$

$$ W = (authority^{secret}, value, rcpt_x, rcpt_y, sn, spendHook, data, blind^{value}, blind^{token}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| Public Input         | Description                                             |
|----------------------|---------------------------------------------------------|
|$authority^{public}_y$| minting authority public key y-coordinate               |
|$authority^{public}_x$| minting authority public key x-coordinate               |
| token                | derived token id                                        |
| $cm^{coin}$          | coin commitment as field element                        |
| $cm^{value}_x$       | x coordinate of supply point commitment                 |
| $cm^{value}_y$       | y coordinate of supply point commitment                 |
| $cm^{token}$         | commitment of tokenId as field element                  |

| witnesses            | Description                                         |
|----------------------|-----------------------------------------------------|
| $authority^{secret}  | minting authority secret key                        |
| value                | token minted supply value                           |
| $rcpt_x$             | token recipient public key x coordinate             |
| $rcpt_y$             | token recipient public key y coordinate             |
| sn                   | serial number for burn coin                         |
| spendHook            | contract related contract                           |
| data                 | data read during spendHook execution                |
| $blind^{value}$      | blinding term for burn value commitment             |
| $blind^{token}$      | blinding term for token id commitment               |
