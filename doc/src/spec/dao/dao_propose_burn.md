# dao propose burn

- TODO why spend hook, and data aren't constrained here?

$$ X = (nullifier, cm^{value}_x, cm^{value}_y, cm^{token}, root,  signature^{public}_x, signature^{public}_y)$$

$$ W = (sk, sn, spendHook, data, value, tokenId, blind^{value}, blind^{token}, pos, path, signature^{secret}) $$

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
