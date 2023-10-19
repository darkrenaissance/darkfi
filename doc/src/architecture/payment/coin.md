# coin

field element commitment to $(pub_x, pub_y, value, token, sn, spendHook, data)$

| coin array           | Description                                       |
|----------------------|---------------------------------------------------|
| $pub_x$              | public key x coordinate                           |
| $pub_y$              | public key y coordinate                           |
| value                | coin value                                        |
| token                | coin token id                                     |
| sn                   | coin serial number                                |
| spendHook            | contract to be executed upon minting that coin    |
| data                 | data required by spendHook                        |
