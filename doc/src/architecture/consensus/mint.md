# leadership mint proof

$$ X = <ep, C, cm_x^{value}, cm_y^{value}> $$
$$ W = <pk_x, pk_y, value, ep, nonce, value_{blind}> $$
$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$

| Public Input | Description                                                |
|--------------|------------------------------------------------------------|
|     ep       | epoch index                                                |
|     C        | coin commitment                                            |
|$cm_x^{value}$| value commitment affine x coordinate                       |
|$cm_y^{value}$| value commitment affine y coordinate                       |

|  Witnesses    | Description                                                |
|---------------|------------------------------------------------------------|
|    $pk_x$     | coin public key pk affine x coordinate                     |
|    $pk_y$     | coin public key pk affine y coordinate                     |
|    value      | coin value $\in \mathbb{Z}$ or u64                         |
|     ep        | epoch index                                                |
|   nonce[^1]       | random nonce derived from previous coin                    |
|$value_{blind}$| blinding scalar for value commitment                       |
-----------------------------------------------------------------------------------


| Functions    | Description                                                |
|--------------|------------------------------------------------------------|
| pk           | commitment to sk                                           |
| C            | $hash(pk_x||pk_y||value||ep|nonce)$                        |
| $cm^{value}$ | commitment to value                                        |

[^1]: if you read this after zerocash which crypsinous is based off, both papers calls nullifiers serial numbers. and serial number is nonce, `sn` in the table below can be called `nullifier` in our contract using zcash sapling terminology which is used in our money contract (sapling contract).
