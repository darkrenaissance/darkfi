# dao propose burn

$$ X = (nullifier, cm^{value}_x, cm^{value}_y, cm^{token}, root,  signature^{public}_x, signature^{public}_y)$$

$$ W = (sk, sn, spendHook[^1], data, value, tokenId, blind^{value}, blind^{token}, pos, path, signature^{secret}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| Public Input         | Description                                             |
|----------------------|---------------------------------------------------------|
| nullifier            | hash of (sk||sn)                                        |
| $cm^{value}_x$       | x coordinate of value point commitment                  |
| $cm^{value}_y$       | y coordinate of value point commitment                  |
| $cm^{token}$         | commitment of tokenId as field element                  |
| root                 | root of commitments tree of coin commitments            |
| data                 | data read during execution of burn spendHook contract   |
| spendHook            | burn related contract                                   |
|$signature^{public}_x$| signature public x coordinate                           |
|$signature^{public}_y$| signature public y coordinate                           |

| witnesses            | Description                                          |
|----------------------|------------------------------------------------------|
| sk                   | proposal coin secret key                             |
| sn                   | proposal coin serial number                          |
| spendHook            | burnt coin spendHook                                 |
| data                 | spendHook contract input data                        |
| value                | proposal coin value                                  |
| tokenId              | proposal token id                                    |
| $blind^{value}$      | proposal value commitment blinding term              |
| $blind^{token}$      | token commitment blinding term                       |
| pos                  | proposal coin leaf position in merkle tree           |
| path                 | proposal coin path in merkle tree                    |
| $signature^{secret}$ | proposal signature secret                            |

[^1]: why spend hook, and data aren't constrained here?
