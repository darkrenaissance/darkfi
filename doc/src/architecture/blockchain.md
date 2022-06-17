# Dynamic Proof of Stake

## Overview

DarkFi's current blockchain is based on Streamlet, a very simple consensus
system based on voting between the participating parties. The blockchain
is currently in the devnet phase and has no concept of a consensus token.

Darkfi is actively working to upgrade its consensus to Ouroboros
Crypsinous, a privacy focused proof-of-stake algorithm. To accommodate
this transition it has designed its data structures to be easy to upgrade.

Below is a specification of how DarkFi's current blockchain achieves
consensus.

## Blockchain

Blockchain $\mathbb{C}$ is a series of epochs: it's a tree of chains,
$C_1$, $C_2$, $\dots$, $C_n$, the chain of the max length in $\mathbb{C}$
is the driving chain C.


## Epoch

An epoch is a multiple of blocks. Some of those blocks might be empty
due to the nature of the leader selection with VRF.


## Genesis block

The first block in the epoch updates the stake for stakeholders, which
influences the weighted random leader selection algorithm. For epoch j,
the pair ($S_j,\eta_j$) is the genesis block's data for n stakeholders
of the blockchain:

$$
S_j=((U_1,v_1^{vrf},v_1^{kes},v_1^{dsig},s_1),\dots,(U_n,v_n^{vrf},v_n^{kes},v_n^{dsig},s_n)
$$ $$ \eta_j \leftarrow \{0,1\}^\lambda $$

<sup><strong>Note that new stakeholders need to wait for the next epoch
to be added to the genesis block</strong></sup>

## Block

A block $\textbf{B}$ is the building block of the blockchain.

Block $B_{i}=(st, d, sl, B_{\pi}, \rho, \sigma_s)$ created for slot i
by a stakeholder, and slot i leader $U_s$:

$$\textbf{\textcolor{red}{st}}: \text{state of the prebvious block,
Hash(head($\mathbb{C}$))}$$ $$\textbf{\textcolor{red}{d}}: \text{data held
by the block}$$ $$\textbf{\textcolor{red}{sl}}: \text{slot id generated
by the beacon}$$ $$\textbf{\textcolor{red}{$B_\pi$}}: \text{proof
the stakeholder ${U_s}$ is the owner, $B_{\pi}=(U_s,y,\pi)$, y,$\pi$
are the output of the VRF}$$ $$\textbf{\textcolor{red}{$\rho$}}:
\text{random seed for vrf, $\rho=(\rho_y,\rho_{\pi})$}$$
$$\textbf{\textcolor{red}{$\sigma_{s}$}}: \text{owner signature on
the block}$$


## Leader selection

At the onset of each slot each a stakeholder needs to verify if it's
the weighted random leader for this slot.

$$y < T_{i}$$ <center><sup><strong>check if VRF output is less than some
threshold </strong></sup></center>

This statement might hold true for zero or more stakeholders, thus
we might end up with multiple leaders for a slot, and other times no
leader. Also note that no one would know who the leader is, how many
leaders are there for the slot, until you receive a signed block with
a proof claiming to be a leader.

$$y = VRF(\eta||sid)$$

<center><sup><strong>$\eta$ is random nonce generated from the blockchain,
$\textbf{sid}$ is block id</strong></sup></center>

$$\phi_{f} = 1 - (1-f)^{\alpha_i}$$ $$T_{i} =
2^{l_{VRF}}\phi_{f}(\alpha_i^j)$$

Note that $\phi_f(1)=f$, $\textbf{f}$: the active slot coefficient is
the probability that a party holding all the stake will be selected to be
a leader. Stakeholder is selected as leader for slot j with probability
$\phi_f(\alpha_i)$, $\alpha_i$ is $U_i$ stake.

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
for the statement $y<T$, $$T=ord(G)\phi_{max}\phi(\alpha)=S\phi(\alpha)$$
$$S=ord(G)\phi_{max}\phi(\alpha)$$
such that S $in Z$
$\phi_{max}=\phi(\alpha_{max})$ where $\alpha_{max}$ is the maximum stake value being $2^{64}$, following from the previous proof that the family of function haveing independent aggregation property is the exponential function $f^\alpha$, and $f \in Z | f>1$, the smallest value satisfying f is $f=2$, then $$\phi_{max} = 2^{2^{64}}$$
note that since $ord(G)<<\phi_{max}$ thus $S<<1$, contradiction.



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

# Protocol

# Appendix

This section gives further details about the structures that will
be used by the protocol. Since the Streamlet consensus protocol will
be used at early stages of development, we created hybrid structures
to enable seamless transition from Stremlet to Ouroboros Crypsinous,
without the need of forking the blockchain.

## Blockchain

| Field    |     Type     |                Description                 |
|----------|--------------|--------------------------------------------|
| `blocks` | `Vec<Block>` | Series of blocks consisting the Blockchain |


## Block

|   Field    |        Type        |            Description            |
|------------|--------------------|-----------------------------------|
| `st`       | `String`           | Previous block hash               |
| `sl`       | `u64`              | Slot UID, generated by the beacon |
| `txs`      | `Vec<Transaction>` | Transactions payload              |
| `metadata` | `Metadata`         | Additional block information      |


## Metadata

|    Field    |         Type        |                  Description                  |
|-------------|---------------------|-----------------------------------------------|
| `om`        | `OuroborosMetadata` | Block information used by Ouroboros consensus |
| `sm`        | `StreamletMetadata` | Block information used by Streamlet consensus |
| `timestamp` | `Timestamp`         | Block creation timestamp                      |


## Ouroboros Metadata

|    Field    |         Type        |                  Description                  |
|-------------|---------------------|-----------------------------------------------|
| `proof`     | `VRFOutput`         | Proof the stakeholder is the block owner      |
| `r`         | `Seed`              | Random seed for the VRF                       |
| `s`         | `Signature`         | Block owner signature                         |


## Streamlet Metadata

|    Field    |         Type        |                  Description                  |
|-------------|---------------------|-----------------------------------------------|
| `votes`     | `Vec<Vote>`         | Epoch votes for the block                     |
| `notarized` | `bool`              | Block notarization flag                       |
| `finalized` | `bool`              | Block finalization flag                       |
