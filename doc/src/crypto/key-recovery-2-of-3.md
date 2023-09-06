# Trustless 2 of 3 Key Recovery Scheme

The aim of this scheme is to enable 3 players to generate a single public key
which can be recovered using any 2 of 3 players. It is trustless and anonymous.

The basic concept relies on the additivity of functions $(f\circ g)(a) = f(a) + g(a)$,
and additive homomorphism of EC points. That way we avoid heavy MPC multiplications
and keep the scheme lightweight.

The values $x_1, \dots, x_6$ are fixed strings known by all players.

## Constructing the Line

Players 1 and 2 each construct their own lines, with the resulting line
being the sum of both. Given any 2 points, we can recover the original line and
hence the secret.

### Player 1 creates line 1

Player 1 creates a random line $\ell_1 = a_1 X + b_1 Y$
and samples points
$$ R_1 = (x_1, y_1), R_2 = (x_2, y_2), R_3 = (x_3, y_3) \in V(\ell_1) $$
Player 1 then sends to:

* Player 2: $x_1, R_2, x_3$
* Player 3: $x_1, x_2, R_3$

### Player 2 creates line 2

Player 2 creates a random line $\ell_2 = a_2 X + b_2 Y$
and samples points
$$ R_4 = (x_4, y_4), R_5 = (x_5, y_5), R_6 = (x_6, y_6) \in V(\ell_2) $$
Player 2 then sends to:

* Player 1: $x_4, R_5, x_6$
* Player 3: $x_4, x_5, R_6$

## Compute Points on $\ell_1 + \ell_2$ for Each Player

* Player 1 computes $Q_1 = R_1 + R_5$
* Player 2 computes $Q_2 = R_2 + R_4$
* Player 3 computes $Q_3 = R_3 + R_6$

Note that $Q_1, Q_2, Q_3 \in V(\ell_1 + \ell_2)$ but $\ell_1 + \ell_2$
is unknown to anyone. However 2 actors can collude to recover the original line.

The secret key (unknown to any player) is:
$$ d = y(Q_1) + y(Q_2) + y(Q_3) $$

## Compute Shared Public Key

Player's 1, 2 and 3 create blinding values $b_1, b_2, b_3$ and sends over
$y(Q_i) + b_i$.

We then compute the blinded public key as
$$ \bar{P} = \sum_{i = 1}^3 (y(Q_i) + b_i)G $$
then each player unblinds $\bar{P}_0 = \bar{P}$ by computing
$\bar{P}_i = \bar{P}_{i - 1} - b_i G$, which gives us
$$ P = \bar{P}_3 = (y(Q_1) + y(Q_2) + y(Q_3))G $$

## Key Recovery

WLOG assume player 1 is recovering the secret key with player 2's $Q_2$.
They compute the line $\ell_1 + \ell_2$ by
$$ m = \frac{y(Q_2) - y(Q_1)}{x(Q_2) - x(Q_1)} $$
$$ L(X) = m(X - x(Q_1)) + y(Q_1) $$
which allows us to compute $Q_3 = (x_2 + x_5, L(x_2 + x_5))$, and so
we are able to recover the secret $d$.


