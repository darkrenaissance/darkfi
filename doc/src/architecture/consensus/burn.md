# leadership burn proof

proof of burn of staked coin.

$$ X = <sn, ep, pk_x, pk_y, root, cm_x^{value}, cm_y^{value}> $$
$$ W = <value, ep, nonce, value_{blind}, sk, \tau ,path> $$
$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| Public Input | Description                                                |
|--------------|------------------------------------------------------------|
|     sn[^1]       | nullifier is hash of nonce nonce, and sk                   |
|     ep       | epoch index                                                |
|    $pk_x$    | coin public key pk affine x coordinate                     |
|    $pk_y$    | coin public key pk affine y coordinate                     |
|     root     | root of coins commitments tree                             |
|$cm_x^{value}$| value commitment affine x coordinate                       |
|$cm_y^{value}$| value commitment affine y coordinate                       |



|  Witnesses   | Description                                                |
|--------------|------------------------------------------------------------|
|    value     | coin value $\in \mathbb{Z}$ or u64                         |
|     ep       | epoch index                                                |
|   nonce[^2]      | random nonce derived from previous coin                    |
| $value_{blind}$  | blinding scalar for value commitment                   |
|     sk       | coin secret key                                            |
|    $\tau$    | C position rooted by root                                  |
|    path      | path of C at position $\tau$                               |



| Functions    | Description                                                |
|--------------|------------------------------------------------------------|
| pk           | commitment to sk                                           |
| C            | $hash(pk_x||pk_y||value||ep|nonce)$                        |
| $cm^{value}$ | commitment to value                                        |


[^1]: if you read this after zerocash which crypsinous is based off, both papers calls nullifiers serial numbers. and serial number is nonce, `sn` in the table below can be called `nullifier` in our contract using zcash sapling terminology which is used in our money contract (sapling contract).
[^2]: if you read this after zerocash which crypsinous is based off, both papers calls nullifiers serial numbers. and serial number is nonce, `nonce` can be called `input/output serial` in our contracts using zcash sapling terminology which is used in our money contract (sapling contract).
