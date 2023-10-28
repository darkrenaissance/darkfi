# Dynamic Proof of Stake

## Overview

The DarkFi blockchain is based off proof of stake  privacy focused Ouroboros Crypsinous,
tunned with a discrete controller to achieve a stable supply.

## Blockchain

Blockchain $\mathbb{C_{loc}}$ is a series of epochs: it's a tree of chains,
$C^1$, $C^2$, $\dots$, $C^n$, the chain ending in a single leader per slot single finalization.

Crypsinous Blockchain is built on top of Zerocash sapling scheme, and Ouroboros Genesis  blockchain.
Each participant $U_p$ stores its own local view of the Blockchain $C_{loc}^{U_p}$.
$C_{loc}$ is a sequence of blocks $B_i$ (i>0), where each $B \in C_{loc}$
$$ B = (tx_{lead},st)$$
$$tx_{lead} = (LEAD, header, txs, stx_{proof})$$
LEAD is a magic word, header is a metadata, and txs is a vector of transaction hash (see appendix).
$stx_{proof}=(cm_{\prime{c}},sn_c,ep,sl,\rho,h,\pi)$
the Block's st is the block data, and h is the hash of that data.
the commitment of the newly created coin is:
$(cm_{c_2},r_{c_2})=COMM(pk^{COIN}||\tau||v_c||\rho_{c_2})$,
$\tau$ is slot timestamp, or index. $sn_c$ is the coin's serial number revealed to spend the coin.
$$sn_c=PRF_{root_{sk}^{COIN}}^{sn}(\rho_c)$$
$$\rho=\eta^{sk_{sl}^{COIN}}$$
$\eta$ is randomness from  random oracle implemented as hash of previous epoch, $\rho$ id derived randomness from $\eta$.  $\pi$ is the NIZK proof of the LEAD statement.


### st transactions
the blockchain view is a chain of blocks, each block $B_j=(tx_{lead},st)$, while $st$ being the merkle tree structure of the validated transactions received through the network, that include transfer, and public transactions.

### LEAD statement
for $x=(cm_{c_2},sn_{c_1},\eta,sl,\rho,h,ptr,\mu_{\rho},\mu_{y},root)$, and
$w=(path,root_{sk^{COIN}},path_{sk^{COIN}},\tau_c,\rho_c,r_{c_1},v,r_{c_2})$
for tuple $(x,w) \in L_{lead}$ iff:

 * $pk^{COIN} = PRF_{root_{sk}^{COIN}}^{pk}(\tau_c)$.
 * $\rho_{c_2}=PRF_{root_{sk_{c_1}}^{COIN}}^{evl}(\rho_{c_1})$.
 note here the nonce of the new coin is deterministically driven from the nonce of the old coin, this works as resistance mechanism to allow the same coin to be eligible for leadership more than once in the same epoch.
 * $\forall i \in \{1,2\} : DeComm(cm_{c_i},pk^{COIN}||v||\rho_{c_i},r_{c_i})=T$.
 * path is a valid Merkle tree path to $cm_{c_1}$ in the tree with the root root.
 * $path_{sk^{COIN}}$ is a valid path to a leaf at position $sl-\tau_c$ in a tree with a root $root_{sk}^{COIN}$.
 * $sn_{c_1}= PRF_{root_{sk}^{COIN}}^{sn}(\rho_{c_1})$
 * $y = \mu_{y}^{root_{sk_{c_1}}^{COIN}||\rho_c}$
 * $\rho = \mu_{\rho}^{root_{sk_{c_1}}^{COIN}||\rho_c}$
 * $y< T(v)$
note that this process involves burning old coin $c_1$, minting new  $c_2$ of the same value + reward.

#### validation rules

validation of proposed lead proof as follows:

* slot index is less than current slot index
* proposal extend from valid fork chain
* transactions doesn't exceed max limit
* signature is valid based off producer public key
* verify block hash
* verify block header hash
* public inputs $\mu_y$, $\mu_{rho}$ are hash of current consensus $\eta$, and current slot
* public inputs of target 2-term approximations $\sigma_1$, $\sigma_2$ are valid given total network stake and controller parameters
* the competing coin nullifier isn't published before to protect against double spending, before burning the coin.
* verify block transactions

<!--
this is now replaced by tx as a contract in zkas
### transfer transaction $tx_{xfer}$
transfer transaction of the pouring mechanism of input: old coin, and public coin, with output: new return change coin, and further recipient coin.  such that input total value $v^{old}_1 + v_{pub} = v^{new}_3 + v^{new}_4$
$$ tx_{xfer} = (TRANSFER,stx_{proof},c_r)$$
$$stx_{proof} = (\{cm_{c_{3}}),cm_{c_{4}}\}),(\{sn_{c_2},{sn_{c_1}}\}),\tau,root,\pi)$$
$c_r$ is forward secure encryption of $stx_{rcpt}=(\rho_{c_3},r_{c_3},v_{c_3})$ to $pk_r$.
the commitment of the new coins $c_3$, $c_4$ is:
$$(cm_{c_3},r_{c_3})=Comm(pk_{pk_s}^{COIN}||\tau||v_{c_3}||\rho_{c_3})$$
$$(cm_{c_4},r_{c_4})=Comm(pk_{pk_r}^{COIN}||\tau||v_{c_4}||\rho_{c_4})$$
also the spend proofs of the old coins $sn_{c_1},sn_{c_2}$ are revealed.


### NIZK proof $\pi$
for the circuit inputs, and witnesses

$$x = (\{cm_{c_3},cm_{c_4}\},\{sn_{c_1},sn_{c_2}\},\tau,root)$$
$$w = (root_{sk_{c_1}^{COIN}},path_{sk_{c_1}^{COIN}},root_{sk_{c_2}^{COIN}},path_{sk_{c_2}^{COIN}},pk_{c_3}^{COIN},pk_{c_4}^{COIN},(\rho_{c_1},r_{c_1},v_1,path_1),(\rho_{c_2},r_{c_2},v_2,path_2),(\rho_{c_1},r_{c_1},v_1,path_1))$$

$\pi$ is a proof for the following transfer statement using zerocash pouring mechanism.

$$\forall_i \in \{1,2\}: pk_{c_i}^{COIN} = PRF_{root_{sk_{c_i}}^{COIN}}^{pk}(1)$$
$$\forall_i \in \{1,\dots,4\} : DeComm(cm_{c_i},pk_{c_i}^{COIN}||v_i||\rho_{c_i},r_{c_i})=T$$
$$v_1+v_2=v_3+v_4$$

$path_1$  is a valid path to  $cm_{c_1}$  in a tree with the root root

$path_2$ is a valid path to  $cm_{c_2}$  in a tree with the root root, $sn_{c_2}=PRF_{root_{sk_{c_1}^{COIN}}}^{zdrv}(\rho_{c_1})$

$$path_{sk_{c_i}^{COIN}} \text{ is a valid path to a leaf at position } \tau \text{ in }, root_{sk_{c_i}^{COIN}} i \in \{1,2\}$$

$$sn_{c_i}=PRF_{root_{sk_{c_i}^{COIN}}}^{sn}(\rho_{c_i}), \forall_i \in \{1,2\}$$

-->



## Epoch

An epoch is a vector of blocks. Some of the  blocks might be empty if there is no winning leader. tokens in stake are constant during the epoch.

## Leader selection

At the onset of each slot each stakeholder needs to verify if it's
the weighted random leader for this slot.

$$y < T_{i}$$ <center><sup><strong> check if the random y output is less than some
threshold </strong></sup></center>

This statement might hold true for zero or more stakeholders, thus
we might end up with multiple leaders for a slot, and other times no
leader. Also note that no node would know the leader identity or how many
leaders are there for the slot, until it receives a signed block with
a proof claiming to be a leader.


<center><sup><strong>$\eta$ is random nonce generated from the blockchain,
$\textbf{sid}$ is block id</strong></sup></center>

$$\phi_{f} = 1 - (1-f)^{\alpha_i}$$ $$T_{i} =
L \phi_{f}(\alpha_i^j)$$

Note that $\phi_f(1)=f$, $\textbf{f}$: the active slot coefficient is
the probability that a party holding all the stake will be selected to be
a leader. Stakeholder is selected as leader for slot j with probability
$\phi_f(\alpha_i)$, $\alpha_i$ is $U_i$ relative stake.

see the appendix for absolute stake aggregation dependent leader selection
family of functions.

### automating f tuning

the stable consensus token supply is maintained by the help of discrete PID controller, that maintain stabilized occurrence of single leader per slot.

#### control lottery f tunning parameter

$$f[k] = f[k-1] + K_1e[k] + K_2e[k-1] + K_3e[k-2]$$

with $k_1 = k_p + K_i + K_d$,  $k_2 = -K_p -2K_d$,  $k_3 = K_d$, and e is the error function.


### target T n-term approximation
target function is approximated to avoid use of power, and division in zk, since no function in the family of functions that have independent aggregation property achieve avoid it (see appendix).

#### target function

 target fuction T: $$ T = L * \phi(\sigma) = L * (1- (1 - f)^{\sigma}) $$
 $\sigma$ is relative stake.
 f is tuning parameter, or the probability of winning have all the stake
 L is field length

#### $\phi(\sigma)$ approximation

 $$\phi(\sigma) = 1 - (1-f)^{\sigma} $$
 $$ = 1 - e^{\sigma ln(1-f)} $$
 $$ = 1 - (1 + \sum_{n=1}^{\infty}\frac{(\sigma ln (1-f))^n}{n!}) $$
 $$ \sigma = \frac{s}{\Sigma} $$
 s is stake, and $\Sigma$ is total stake.

#### target T n term approximation

 $$ k = L ln (1-f)^1 $$
 $$ k^{'n} =  L ln (1-f)^n $$
 $$ T = -[k\sigma + \frac{k^{''}}{2!} \sigma^2 + \dots +\frac{ k^{'n}}{n!}\sigma^n] $$
 $$  = -[\frac{k}{\Sigma}s + \frac{k^{''}}{\Sigma^2 2!} s^2 + \dots +\frac{k^{'n}}{\Sigma^n n!} s^n] $$

#### comparison of original target to approximation

![approximation comparison to the original](https://github.com/darkrenaissance/darkfi/blob/master/script/research/crypsinous/linearindependence/target.png?raw=true)


# Appendix

This section gives further details about the structures that will
be used by the protocol.

## Blockchain

| Field    |     Type     |                Description                 |
|----------|--------------|--------------------------------------------|
| `blocks` | `Vec<Block>` | Series of blocks consisting the Blockchain |


## Header

|   Field     |        Type        |            Description                     |
|-------------|--------------------|--------------------------------------------|
| `version`   | `u8`               | Version                                    |
| `previous`  | `blake3Hash`       | Previous block hash                        |
| `epoch`     | `u64`              | Epoch                                      |
| `slot`      | `u64`              | Slot UID                                   |
| `timestamp` | `Timestamp`        | Block creation timestamp                   |
| `root`      | `MerkleRoot`       | Root of the transaction hashes merkle tree |


## Block

|   Field     |        Type       |            Description             |
|-------------|-------------------|------------------------------------|
| `magic`     | `u8`              | Magic bytes                        |
| `header`    | `blake3Hash`      | Header hash                        |
| `txs`       | `Vec<blake3Hash>` | Transaction hashes                 |
| `lead_info` | `LeadInfo`        | Block leader information           |

## LeadInfo

| Field           | Type                | Description                                         |
|-----------------|---------------------|-----------------------------------------------------|
| `signature`     | `Signature`         | Block owner signature                               |
| `public_inputs` | `Vec<pallas::Base>` | Nizk proof public inputs                            |
| `serial_number` | `pallas::Base`      | competing coin's nullifier                          |
| `eta`           | `[u8; 32]`          | randomness from the previous epoch                  |
| `proof`         | `Vec<u8>`           | Nizk $\pi$ Proof the stakeholder is the block owner |
| `offset`        | `u64`               | Slot offset block producer used                     |
| `leaders`       | `u64`               | Block producer leaders count                        |

## Public Inputs
| Field        | Type           | Description                                           |
|--------------|----------------|-------------------------------------------------------|
| `pk`         | `pallas::Base` | burnt coin public key                                 |
| `c1_cm_x`    | `pallas::Base` | burnt coin commitment x coordinate                    |
| `c1_cm_y`    | `pallas::Base` | burnt coin commitment y coordinate                    |
| `c2_cm_x`    | `pallas::Base` | minted coin commitment x coordinate                   |
| `c2_cm_y`    | `pallas::Base` | minted coin commitment y coordinate                   |
| `cm1_root`   | `pallas::Base` | root of burnt coin commitment in burnt merkle tree    |
| `c1_sk_root` | `pallas::Base` | burn coin secret key                                  |
| `sn`         | `pallas::Base` | burnt coin spending nullifier                         |
| `y_mu`       | `pallas::Base` | random seed base from blockchain                      |
| `y`          | `pallas::Base` | hash of random seed, and `y_mu`, used in lottery      |
| `rho_mu`     | `pallas::Base` | random seed base from blockchain                      |
| `rho`        | `pallas::Base` | hash of random seed and `rho_mu` to constrain lottery |
| `sigma1`     | `pallas::Base` | first term in 2-terms target approximation.           |
| `sigma2`     | `pallas::Base` | second term in 2-terms target approximation.          |


### Linear family functions

In the previous leader selection function, it has the unique
property of independent aggregation of the stakes, meaning the
property of a leader winning leadership with stakes $\sigma$
is independent of whether the stakeholder would act as a pool
of stakes, or distributed stakes on competing coins.  "one minus
the probability" of winning leadership with aggregated stakes is
$1-\phi(\sum_{i}\sigma_i)=1-(1+(1-f)^{\sigma_i})=-(1-f)^{\sum_{i}\sigma_i}$,
the joint "one minus probability" of all the stakes (each with
probability $\phi(\sigma_i))$ winning aggregated winning the leadership
$\prod_{i}^{n}(1-\phi(\sigma_i))=-(1-f)^{\sum_i(\sigma_i)}$ thus: $$
1-\phi(\sum_{i}\sigma_i) =\prod_{i}^{n}(1-\phi(\sigma_i)) $$

A non-exponential  linear leader selection can be:

$$y < T $$ $$y = 2^lk \mid 0 \le k \le 1$$ $$T = 2^l\phi(v)$$ $$
\phi(v)=\frac{1}{v_{max+}+c}v  \mid c \in \mathbb{Z}$$

#### Dependent aggregation

Linear leader selection has the dependent aggregation property, meaning
it's favorable to compete in pools with sum of the stakes over aggregated
stakes of distributed stakes:

$$\phi(\sum_{i}{\sigma_i})>\prod_{i}^{n}{\sigma_i}$$
$$\sum_{i}{\sigma_i}>(\frac{1}{v_{max}+c})^{n-1}v_1v_2 \dots
v_n$$ let's assume the stakes are divided to stakes of value
$\sigma_i=1$ for $\Sigma>1 \in \mathbb{Z}$, $\sum_{i}{\sigma_i}=V$
$$V>(\frac{1}{v_{max}+c})^{n-1}$$ note that $(\frac{1}{v_{max}+c})^{n-1}
< 1, V>1$, thus competing with single coin of the sum of stakes held by
the stakeholder is favorable.

#### Scalar linear aggregation dependent leader selection

A target function T with scalar coefficients can be formalized as
$$T=2^lk\phi(\Sigma)=2^l(\frac{1}{v_{max}+c})\Sigma$$ let's assume
$v_{max}=2^v$, and $c=0$ then: $$T=2^lk\phi(\Sigma)=2^{l-v}\Sigma$$
then the lead statement is $$y<2^{l-v}\Sigma$$ for example for a group
order or l=    24 bits, and maximum value of $v_{max}=2^{10}$, then
lead statement: $$y<2^{14}\Sigma$$

#### Competing max value coins

For a stakeholder with $nv_{max}$ absolute stake, $\mid n \in \mathbb{Z}$
it's advantageous for the stakeholder to distribute stakes on $n$
competing coins.

#### Inverse functions

Inverse lead selection functions doesn't require maximum stake, most
suitable for absolute stake, it has the disadvantage that it's inflating
with increasing rate as time goes on, but it can be function of the
inverse of the slot to control the increasing frequency of winning
leadership.

#### Leader selection without maximum stake upper limit

The inverse leader selection without maximum stake value can be
$\phi(v)=\frac{v}{v+c} \mid c  > 1$ and inversely proportional
with probability of winning leadership, let it be called leadership
coefficient.


#### Decaying linear leader selection

As the time goes one, and stakes increase, this means the combined stakes
of all stakeholders increases the probability of winning leadership
in next slots leading to more leaders at a single slot, to maintain,
or to be more general to control this frequency of leaders per slot, c
(the leadership coefficient) need to be function of the slot $sl$, i.e
$c(sl) = \frac{sl}{R}$ where $R$ is epoch size (number of slots in epoch).

#### Pairing leader selection independent aggregation function

The only family of functions $\phi(\alpha)$ that are isomorphic
to summation on multiplication $\phi(\alpha_1+\alpha_2)
= \phi(\alpha_1)\phi(\alpha_2)$(having the independent aggregation
property) is the exponential function, and since it's impossible to
implement in plonk,  a re-formalization of the lead statement using
pairing that is isomorphic to summation on multiplication is an option.

Let's assume $\phi$ is isomorphic function
between multiplication and addition, $\phi(\alpha) =
\phi(\frac{\alpha}{2})\phi(\frac{\alpha}{2})=\phi(\frac{\alpha}{2})^2$,
thus:
$$\phi(\alpha)=\underbrace{\phi(1)\dots\phi(1)}_\text{$\alpha$}=\phi(1)^\alpha$$
then the only family of functions $\phi : \mathbb{R} \rightarrow
\mathbb{R}$ satisfying this is the exponential function
$$\phi(\alpha)=c^{\alpha} \mid c  \in \mathbb{R}$$

#### no solution for the lead statement parameters, and constants $S,f, \alpha$ defined over group of integers.

assume there is a solution for the lead statement parameters and constants $S, f, \alpha$ defined over group of integers.
for the statement $y<T$, $$T=L\phi_{max}\phi(\alpha)=S\phi(\alpha)$$
$$S=ord(G)\phi_{max}\phi(\alpha)$$
such that S $in Z$
$\phi_{max}=\phi(\alpha_{max})$ where $\alpha_{max}$ is the maximum stake value being $2^{64}$, following from the previous proof that the family of function having independent aggregation property is the exponential function $f^\alpha$, and $f \in Z | f>1$, the smallest value satisfying f is $f=2$, then $$\phi_{max} = 2^{2^{64}}$$
note that since $ord(G)<<\phi_{max}$ thus $S<<1$, contradiction.


### Leaky non-resettable beacon

Built on top of globally synchronized clock, that leaks the nonce $\eta$
of the next epoch a head of time (thus called leaky), non-resettable
in the sense that the random nonce is deterministic at slot s, while
assuring security against adversary controlling some stakeholders.

For an epoch j, the nonce $\eta_j$ is calculated by hash function H, as:

$$\eta_j = H(\eta_{j-1}||j||v)$$

v is the concatenation of the value $\rho$ in all blocks from the
beginning of epoch $e_{i-1}$ to the slot with timestamp up to $(j-2)R +
\frac{16k}{1+\epsilon}$, note that k is a persistence security parameter,
R is the epoch length in terms of slots.

### toward better decentralization in ouroboros

the randomization of the leader selection at each slot is hinged on the random $y$, $\mu_y$, $\rho_c$, those three values are derived from $\eta$, and root of the secret keys, the root of the secret keys for each stakeholder can be sampled, and derived beforehand, but $\eta$ is a response to global random oracle query, so it's security is hinged on $\textit{centralized global random node}$.

#### solution

to break this centralization, a decentralized emulation of $G_{ro}$ functionality for calculation of: $\eta_i=PRF^{G_{ro}}_{\eta_{i-1}}(\psi)$
$$\psi   =  hash(tx^{ep}_{0})$$
$$\eta_0 =  hash(\mathrm{"let\; there\; be\; dark!"})$$
note that first transaction in the block, is the proof transaction.
