# freeze token

burn minted coins

$$ X = (authority^{public}_x, authority^{public}_y, token) $$

$$ W = (authority^{secret}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| Public Input         | Description                                             |
|----------------------|---------------------------------------------------------|
|$authority^{public}_y$| minting authority public key y-coordinate               |
|$authority^{public}_x$| minting authority public key x-coordinate               |
| token                | derived token id                                        |

| witnesses            | Description                                         |
|----------------------|-----------------------------------------------------|
| $authority^{secret}  | minting authority secret key                        |
