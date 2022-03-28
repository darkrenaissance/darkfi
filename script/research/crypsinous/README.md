this is an effort to break down the building blocks of crypsinous blockchain

# Crypsinous blockchain
Crypsinous Blockchain is built on top of Zerocash sapling scheme, and Ouroboros Genesis  blockchain.
Each part $U_p$ stores it's own local view of the Blockchain $C_{loc}^{U_p}$.
$C_{loc}$ is a sequence of blocks $B_i$ (i>0), where each $B \in C_{loc}$
$$ B = (tx_{lead},st)$$
$$tx_{lead} = (LEAD,st\overrightarrow{x}_{ref},stx_{proof})$$
$st\overrightarrow{x}_{ref}$ it's a vector of $tx_{lead}$ that aren't yet in $C_{loc}$.
$stx_{proof}=(cm_{\prime{c}},sn_c,ep,sl,\rho,h,ptr,\pi)$
the Blocks' $\emph{st}$ is the block data, and $\emph{h}$ is the hash of that data.
the commitment of the newly created coin is:
$(cm_{\prime{c}},r_{\prime{c}})=COMM(pk^{COIN}||\tau||v_c||\rho_{\prime{c}})$,
$\tau$ is the clock current time. \emph{$sn_c$} is the coin's serial number revealed to spend the coin.
$$sn_c=PRF_{root_{sk}^{COIN}}^{sn}(\rho_c)$$
$$\rho=\eta^{sk_{sl}^{COIN}}$$
$\eta$ is is from random oracle evaluated at $(Nonce||\eta_{ep}||sl)$, $\rho$ is the following epoch's seed. $\emph{ptr}$ is the hash of the previous block, $\pi$ is the NIZK proof of the LEAD statement.

## LEAD statement
for $x=(cm_{c_2},sn_{c_1},\eta,sl,\rho,h,ptr,\mu_{\rho},\mu_{y},root)$, and
$w=(path,root_{sk^{COIN}},path_{sk^{COIN}},\tau_c,\rho_c,r_{c_1},v,r_{c_2})$
for tuple $(x,w) \in L_{lead}$ iff:

 * $pk^{COIN} = PRF_{root_{sk}^{COIN}}^{pk}(\tau_c)$.
 * $\rho_{c_2}=PRF_{root_{sk_{c_1}}^{COIN}}^{evl}(\rho_{c_1})$.
 * $\forall i \in \{1,2\} : DeComm(cm_{c_i},pk^{COIN}||v||\rho_{c_i},r_{c_i})=T$.
 * \emph{path} is a valid Merkle tree path to $cm_{c_1}$ in the tree with the root \emph{root}.
 * \emph{$path_{sk^{COIN}}$} is a valid path to a leaf at position $sl-\tau_c$ in a tree with a root $root_{sk}^{COIN}$.
 * $sn_{c_1}= PRF_{root_{sk}^{COIN}}^{sn}(\rho_{c_1})$
 * $y = \mu_{y}^{root_{sk_{c_1}}^{COIN}||\rho_c}$
 * $\rho = \mu_{\rho}^{root_{sk_{c_1}}^{COIN}||\rho_c}$
 * $y< ord(G)\phi_f(v)$


## spend proof
## transfer proof

# Performance
since Crypsinous is based of sapling scheme, the performance relative to zerocash sapling scheme is that number of constraints in the PRF is improved by replacing sha256 (83,712 constraints) by pederson commitment (2,542 constraints), but on the other hand the proving take twice that of the sapling.

# Appendix

## PRF
pseudo random function $f(x)$ is defined as elliptic curve encryption over the group $<g>$ of random output as \emph{elligator} curves of poseidon hash H

### $PRF^{sn}$:

$$ PRF^{sn}_{root_{sk}^{COIN}}(x)= H(x||0b00)^{root_{sk}^{COIN}}$$

### $PRF^{pk}$:

$$ PRF^{pk}_{root_{sk}^{COIN}}(x)= H(x||0b01)^{root_{sk}^{COIN}}$$

### $PRF^{evl}$:

$$ PRF^{evl}_{root_{sk}^{COIN}}(x)= H(x||0b10)^{root_{sk}^{COIN}}$$

## $root^{COIN}_{sk}(\tau)$
the root in the merkle tree of the current epoch's coins secret keys, at the onset of the epoch, the initial slot's coin's secret key

## Comm,DeComm
the equivocal commitment $(cm,r) \leftarrow Comm(m)$, while the de-commitment is $DeComm(cm,m,r)\rightarrow True$ if it verifies. the commitment can be implemented as blinded encryption of m, as follows $$mG_1 + rG_2$$
    for random groups $G_1$, $G_2$, or as $PRF_{r}^{comm}(m)$
    $$ PRF^{comm}_{r}(m)= H(m||0b11)^{r}$$

# references
[https://eprint.iacr.org/2018/1132.pdf](https://eprint.iacr.org/2018/1132.pdf)
