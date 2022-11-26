# Dynamic Proof of Stake

## Overview

Darkfi is based off Ouroboros Crypsinous, a privacy focused proof-of-stake algorithm.
Below you may find the technical specifications of DarkFi's blockchain implementation.

## Blockchain

Blockchain $\mathbb{C_{loc}}$ is a series of epochs: it's a tree of chains,
$C^1$, $C^2$, $\dots$, $C^n$, the chain of the max length in $\mathbb{C_{loc}}$
is the driving chain $C_{loc}$.

Crypsinous Blockchain is built on top of Zerocash sapling scheme, and Ouroboros Genesis  blockchain.
Each part $U_p$ stores it's own local view of the Blockchain $C_{loc}^{U_p}$.
$C_{loc}$ is a sequence of blocks $B_i$ (i>0), where each $B \in C_{loc}$
$$ B = (tx_{lead},st)$$
$$tx_{lead} = (LEAD,st\overrightarrow{x}_{ref},stx_{proof})$$
$st\overrightarrow{x}_{ref}$ it's a vector of $tx_{lead}$ that aren't yet in $C_{loc}$.
$stx_{proof}=(cm_{\prime{c}},sn_c,ep,sl,\rho,h,ptr,\pi)$
the Blocks' $\emph{st}$ is the block data, and $\emph{h}$ is the hash of that data.
the commitment of the newly created coin is:
$(cm_{c_2},r_{c_2})=COMM(pk^{COIN}||\tau||v_c||\rho_{c_2})$,
$\tau$ is the clock current time. \emph{$sn_c$} is the coin's serial number revealed to spend the coin.
$$sn_c=PRF_{root_{sk}^{COIN}}^{sn}(\rho_c)$$
$$\rho=\eta^{sk_{sl}^{COIN}}$$
$\eta$ is is from random oracle evaluated at $(Nonce||\eta_{ep}||sl)$, $\rho$ is the following epoch's seed. $\emph{ptr}$ is the hash of the previous block, $\pi$ is the NIZK proof of the LEAD statement.

## st transactions
the blockchain view is a chain of blocks, each block $B_j=(tx_{lead},st)$, while st being the merkle tree structure of the validated transactions received through the network, that include transfer, and public transactions.

## LEAD statement
for $x=(cm_{c_2},sn_{c_1},\eta,sl,\rho,h,ptr,\mu_{\rho},\mu_{y},root)$, and
$w=(path,root_{sk^{COIN}},path_{sk^{COIN}},\tau_c,\rho_c,r_{c_1},v,r_{c_2})$
for tuple $(x,w) \in L_{lead}$ iff:

 * $pk^{COIN} = PRF_{root_{sk}^{COIN}}^{pk}(\tau_c)$.
 * $\rho_{c_2}=PRF_{root_{sk_{c_1}}^{COIN}}^{evl}(\rho_{c_1})$.
 note here the nonce of the new coin is deterministically driven from the nonce of the old coin, this works as resistance mechanism to allow the same coin to be eligible for leadership more than once in the same epoch.
 * $\forall i \in \{1,2\} : DeComm(cm_{c_i},pk^{COIN}||v||\rho_{c_i},r_{c_i})=T$.
 * \emph{path} is a valid Merkle tree path to $cm_{c_1}$ in the tree with the root \emph{root}.
 * \emph{$path_{sk^{COIN}}$} is a valid path to a leaf at position $sl-\tau_c$ in a tree with a root $root_{sk}^{COIN}$.
 * $sn_{c_1}= PRF_{root_{sk}^{COIN}}^{sn}(\rho_{c_1})$
 * $y = \mu_{y}^{root_{sk_{c_1}}^{COIN}||\rho_c}$
 * $\rho = \mu_{\rho}^{root_{sk_{c_1}}^{COIN}||\rho_c}$
 * $y< ord(G)\phi_f(v)$
note that this process involves renewing the old coin $c_1$ who's serial number gets revealed(proof of spending), becoming an input, to $c_2$ of the same value,


## transfer transaction $tx_{xfer}$
transfer transaction of the pouring mechanism of input: old coin, and public coin, with output: new return change coin, and further recipient coin.  such that input total value $v^{old}_1 + v_{pub} = v^{new}_3 + v^{new}_4$
$$ tx_{xfer} = (TRANSFER,stx_{proof},c_r)$$
$$stx_{proof} = (\{cm_{c_{3}}),cm_{c_{4}}\}),(\{sn_{c_2},{sn_{c_1}}\}),\tau,root,\pi)$$
$c_r$ is forward secure encryption of $stx_{rcpt}=(\rho_{c_3},r_{c_3},v_{c_3})$ to $pk_r$.
the commitment of the new coins $c_3$, $c_4$ is:
$$(cm_{c_3},r_{c_3})=Comm(pk_{pk_s}^{COIN}||\tau||v_{c_3}||\rho_{c_3})$$
$$(cm_{c_4},r_{c_4})=Comm(pk_{pk_r}^{COIN}||\tau||v_{c_4}||\rho_{c_4})$$

### spend proof
the spend proofs of the old coins $sn_{c_1},sn_{c_2}$ are revealed.

### NIZK proof $\pi$
for the circuit inputs, and witnesses

$$x = (\{cm_{c_3},cm_{c_4}\},\{sn_{c_1},sn_{c_2}\},\tau,root)$$
$$w = (root_{sk_{c_1}^{COIN}},path_{sk_{c_1}^{COIN}},root_{sk_{c_2}^{COIN}},path_{sk_{c_2}^{COIN}},pk_{c_3}^{COIN},pk_{c_4}^{COIN},(\rho_{c_1},r_{c_1},v_1,path_1),(\rho_{c_2},r_{c_2},v_2,path_2),(\rho_{c_1},r_{c_1},v_1,path_1))$$

$\pi$ is a proof for the following transfer statement using zerocash pouring mechanism.

$$\forall_i \in \{1,2\}: pk_{c_i}^{COIN} = PRF_{root_{sk_{c_i}}^{COIN}}^{pk}(1)$$
$$\forall_i \in \{1,\dots,4\} : DeComm(cm_{c_i},pk_{c_i}^{COIN}||v_i||\rho_{c_i},r_{c_i})=T$$
$$v_1+v_2=v_3+v_4$$

$$path_1\text{ is a valid path to } cm_{c_1} \text{ in a tree with the root} \emph{ root}$$

$$path_2\text{ is a valid path to } cm_{c_2} \text{ in a tree with the root} \emph{ root}, sn_{c_2}=PRF_{root_{sk_{c_1}^{COIN}}}^{zdrv}(\rho_{c_1})$$

$$path_{sk_{c_i}^{COIN}} \text{ is a valid path to a leaf at position } \tau \text{ in }, root_{sk_{c_i}^{COIN}} i \in \{1,2\}$$

$$sn_{c_i}=PRF_{root_{sk_{c_i}^{COIN}}}^{sn}(\rho_{c_i}), \forall_i \in \{1,2\}$$

# toward better decentralization in ouroboros

the randomization of the leader selection at each slot is hinged on the random $y$, $\mu_y$, $\rho_c$, those three values are dervied from $\eta$, and root of the secret keys, the root of the secret keys for each stakeholder can be sampled, and derived beforehand, but $\eta$ is a response to global random oracle, so the whole security of the leader selection is hinged on $\textit{centralized global random node}$.

## solution

to break this centeralization, a decentralized emulation of $G_{ro}$ functionality for calculation of: $\eta_i=PRF^{G_{ro}}_{\eta_{i-1}}(\psi)$
$$\psi=hash(tx^{ep}_{0})$$
$$\eta_0=hash("let there be dark!")$$
note that first transaction in the block, is the proof transaction.


## Epoch

An epoch is a vector of blocks. Some of the  blocks might be empty if there is no winnig leader.



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

The following are absolute stake aggregation dependent leader selection
family of functions.

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

### Inverse functions

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
$\phi_{max}=\phi(\alpha_{max})$ where $\alpha_{max}$ is the maximum stake value being $2^{64}$, following from the previous proof that the family of function haveing independent aggregation property is the exponential function $f^\alpha$, and $f \in Z | f>1$, the smallest value satisfying f is $f=2$, then $$\phi_{max} = 2^{2^{64}}$$
note that since $ord(G)<<\phi_{max}$ thus $S<<1$, contradiction.

### target T n term approximation
- s is stake, and $\Sigma$ is total stake.
- $$ \sigma = \frac{s}{\Sigma} $$
- $$ T  = -[\frac{k}{\Sigma}s + \frac{k^{''}}{\Sigma^2 2!} s^2 + \dots +\frac{k^{'n}}{\Sigma^n n!} s^n] $$


## Leaky non-resettable beacon

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
