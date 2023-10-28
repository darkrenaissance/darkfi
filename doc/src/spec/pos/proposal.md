# Proposal


$$ X = (sn, ep, pk_x, pk_y, root, cm_x^{value}, cm_y^{value}, reward, cm_x^{value^{out}}, cm_y^{value^{out}}, C, \mu_y, y, \mu_{\rho}, \rho,\sigma_1, \sigma_2, headstart) $$
$$ W = (sk, nonce, value, ep, reward, value_{blind}, \tau, path, value_{blind}^{out}, \mu_y, \mu_{\rho}, \sigma1, \sigma2, headstart) $$
$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$


| Public Input       | Description                                                |
|--------------------|------------------------------------------------------------|
|     sn[^1]         | nullifier is hash of nonce nonce, and sk                   |
|     ep             | epoch index                                                |
|    $pk_x$          | coin public key pk affine x coordinate                     |
|    $pk_y$          | coin public key pk affine y coordinate                     |
|     root           | root of coins commitments tree                             |
|$cm_x^{value}$      | value commitment affine x coordinate                       |
|$cm_y^{value}$      | value commitment affine y coordinate                       |
| reward             | lottery reward value $\in \mathbb{Z}$ of type u64          |
|$cm_x^{value^{out}}$| value commitment affine x coordinate                       |
|$cm_y^{value^{out}}$| value commitment affine y coordinate                       |
|     $C^{out}$      | coin commitment                                            |
| $\mu_y$            | random, deterministic PRF output                           |
| $\mu_{\rho}$       | random, deterministic PRF output                           |
| $\rho$             | on-chain entropy as hash of nonce, and $\mu_{\rho}$        |
| $\sigma_1$         | target function approximation first term coefficient       |
| $\sigma_2$         | target function approximation second term coefficient      |
-----------------------------------------------------------------------------------



|  Witnesses          | Description                                                |
|---------------------|------------------------------------------------------------|
| sk                  | coin secret key derived from previous coin sk              |
|   nonce[^2]         | random nonce derived from previous coin                    |
|    value            | coin value $\in \mathbb{Z}$ or u64                         |
|     ep              | epoch index                                                |
| reward              | lottery reward value $\in \mathbb{Z}$ of type u64          |
| $value_{blind}$     | blinding scalar for value commitment                       |
|    $\tau$           | C position rooted by root                                  |
|    path             | path of C at position $\tau$                               |
|$value_{blind}^{out}$| blinding scalar for value commitment of newly minted coin  |
| $\mu_y$             | random, deterministic PRF output                           |
| $\mu_{\rho}$        | random, deterministic PRF output                           |
| $\sigma_1$          | target function approximation first term coefficient       |
| $\sigma_2$          | target function approximation second term coefficient      |
| headstart           | competitive advantage added to target T                    |
-----------------------------------------------------------------------------------

Table: if you read this after zerocash which crypsinous is based off, both papers calls nullifiers serial numbers. and serial number is nonce, `sn` in the table below can be called `nullifier` in our contract, similarly `nonce` can be called `input/output serial` using zcash sapling terminology which is used in our money contract (sapling contract).



| Functions    | Description                                                |
|--------------|------------------------------------------------------------|
| $value^{out}$| value + reward                                             |
| $nonce^{out}$| $hash(sk||nonce)$                                          |
| $sk^{out}$   | $hash(sk)$                                                 |
| $pk^{out}$   | commitment to $sk^{out}$                                   |
| $C^{out}$    | $hash(pk_x^{out}||pk_y^{out}||value^{out}||ep|nonce^{out})$|
| $cm^{value}$ | commitment to $value^{out}$                                |
