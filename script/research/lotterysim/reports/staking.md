---
title: tokens in stake
author: ertosns
date: 24/3/2023
---

# Staking in darkfi blockchain

The leadership winning mechanism is based off Ouroborous Crypsinous
with some modifications. A stakeholder wins if some random value $y$,
specific to the stakeholder and derived from the blockchain, is less
than target value $T$. The probability of winning is quasi linear with
the relative stake.

## Least amount of DRK token required for staking

Accuracy of single leader per slot is affected by percentage of total
DRK tokens in stake, in fact the relation is logarithmic.

Assume community $C$ owns 100% of DRK tokens.

The probability of $C$ winning the lottery at any slot is defined as:

\begin{align*}
P(C=lead) &= y < 1 -(1-f)^\alpha \\
          &= y < 1 -(1-f) \\
          &= y < f
\end{align*}

In our case f is targetting single leader per slot. An emulation of
the leader election mechanism with PID controllers shows that f is
oscillating around ~0.65 (depending on ration of tokens in stake).

Then,

\begin{align*}
P(C=lead)~=0.35
\end{align*}

## Linear independence

Given the linear independence property of the target function T, the
probability of a node winning leadership at any slot with S staked tokens
is the same as the probability of N nodes winning leadership at same slot,
with same stake S for any S, N values.

### Example

If the probability of stakeholder owning 0.1% of the tokens is 0.03,
then the probability of a pool consisting of stakeholders owning 0.1%
of tokens is also 0.03.

# Tokens in stake

The probability of a pool of N% stake to win the leadership at any slot is:

\begin{align*}
\frac{N}{100}*P(C=lead)
\end{align*}


![alt text](https://github.com/darkrenaissance/darkfi/blob/master/script/research/lotterysim/reports/stake.png?raw=true)

## Example

Assume $P(C=lead)=33%$, then if only 10% of the total network token
is staked the probability of having a single leader per slot is 0.03,
or accuracy of 3%.

## Ratio of staked tokens in different networks

| network    | staked ratio |
-------------|---------------
| Etherum    |   16%        |
| Cardano    |   69%        |
| Solana     |   71%        |
| Bnb chain  |   16%        |
| Polygon    |   40%        |
| Polkadot   |   47%        |

# Stake privacy leakage
From the graph above, and as a consequence of the linear independence
property the accuracy of the controller leaks the percentage of token
in stake.

## Fix stake privacy leakage
Instant finality mechanism as khonsu would prevent such leakage.
