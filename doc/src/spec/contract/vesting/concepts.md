# Concepts

The vesting process is divided in a few steps that are outlined below:

* **Vest:** a vesting configuration is submitted to the blockchain.
* **Withdraw:** vestee can withdraw some amount of their vested coin.
* **Forfeit:** vesting authority forfeits a specific vesting.
* **Exec:** overlay function over `Withdraw` and `Forfeit`, used as the
  spend hook binding the coins to these contract calls.

## Vest

Vesting authority submits a vesting configuration on-chain, along with
a chained transfer call burning the to-be-vested input coins, which
must be for the same token, and minting a new coin with their total
amount for a shared secret address, using the contracts' `Exec` call
spend-hook, effectivelly becoming usable only withing the vesting
contract context.

> Note: There is a limitation though; the vested coin can only be
> controlled by a single address. To overcome this, when the vesting
> authority creates the vested coin, instead of using the recipients
> actual address, it generates a new random one, which is shared with
> the vestee(in a safe off-chain manner), so the coin can be controlled
> by both parties.

> Note: Since vesting requires a 1-1 vested coin to config matching, it
> means that those coins can be tracked by the vesting configuration
> bulla. It doesn't affect rest anonimity, since it's specific to the
> vesting contract flow and no other information can be derived by it.

## Withdraw

After some time has passed, the vestee can withdraw some amount from
their vested coin, which is done by a chained transfer call burning the
existing vested coin and creating two output coins; one which will be a
normal coin for a withdrawal address, and a second one representing the
remaining balance using the spend hook of the vesting contract. This
call is responsible to define the available-to-withdraw amount, along
with ensuring the chained transfer call second output correctly
represents the remaining balance vested coin. We don't care if that
becomes a zero value coin, since it won't be usable anymore, and we
want all our outputs to look the same, so the final withdrawl cannot be
tracked.

> Note: A medatada leak exists though; the final withdrawl cannot be
> conventionally tracked, since nobody but the parties knows when the
> remaining balance coin becomes a zero value one, but someone tracking
> all the withdrawls calls for a specific vesting configuration can
> assume that the vesting has ended, if no other withdrawl is observed
> after some time period. Still, they can't prove that the vesting has
> concluded without access to the vesting information and/or the shared
> secret address.

### Vesting formulas

Let $E, S, V, C, T$ be the vesting configuration parameters as defined
in [Vesting Configuration](model.md#vesting-configuration).

Let $t_0 = \t{BlockWindow} \in [0, 2^{64})$ be the current blockwindow
as defined in [Blockwindow](model.md#blockwindow).

Let $Bv \ in [0, 2^{64})$ be the burned coin value.

The core formula to compute amounts corresponding to the current block
window is:

$$ \text{CurrentBlockwindow} = \text{CondSelect}(\text{BlockwindowCond}, t_0, E) $$
$$ \text{BlockwindowsPassed} = \text{CurrentBlockwindow} - S $$
$$ \text{Available} = (\text{BlockwindowsPassed} * V) + C $$
$$ \text{Withdrawn} = T - Bv $$
$$ \text{WithdrawCoinValue} = \text{Available} - \text{Withdrawn} $$
$$ \text{VestingChangeValue} = T - (\text{Withdrawn} + \text{WithdrawCoinValue}) $$

The vesting schedule model says that any blockwindow $t$ where
$S \leq t \leq E$, the total amount that should have been unlocked is:

$$ \text{Available}(t) = (t - S) * V + C $$

And we know from the vest proof's constraint that $T = (E-S) * V + C$,
so $\text{Available}(E) = T$. The schedule is linear between $S$ and $E$ with
a cliff $C$ at the start.

The burned vested coin has value $Bv$ which represents the remaining
balance in the vested coin. Initially (right after vest) $Bv = T$.
After each withdrawal it shrinks.

So "total withdrawn so far" is $T - Bv$ and the formula computes how
much new value the vestee can take:

$$ \text{WithdrawCoinValue} = \text{Available} - (T - Bv) = \text{Available} - T + Bv $$
$$ \text{VestingChangeValue} = Bv - \text{WithdrawCoinValue} $$

Concrete example:

Let:

$$ T = 1000 $$
$$ C = 100 $$
$$ S = 10 $$
$$ E = 20 $$
$$ V = 90 $$
$$ (20 - 10) * 90 + 100 = 1000 $$

First withdrawal at $t = 12$ with $Bv = 100$ as the initial vested coin:

$$ \text{Available} = (12 - 10) * 90 + 100 = 280 $$
$$ \text{Withdrawn} = 1000 - 1000 = 0 $$
$$ \text{WithdrawCoinValue} = 280 - 0 = 280 $$
$$ \text{VestingChangeValue} = 1000 - 280 = 720 $$
$$ \text{Conservation: } 280 + 720 = 1000 = Bv $$

Second withdrawal at $t = 15$ with $Bv = 720$ from previous change coin:

$$ \text{Available} = (15 - 10) * 90 + 100 = 550 $$
$$ \text{Withdrawn} = 1000 - 720 = 280 $$
$$ \text{WithdrawCoinValue} = 550 - 280 = 270 $$
$$ \text{VestingChangeValue} = 720 - 270 = 450 $$
$$ \text{Conservation: } 270 + 450 = 720 = Bv $$
$$ \text{Cumulative withdrawn: } 280 + 270 = 550 = \text{Available}(15) $$

Final withdrawal at $t = 20$ (end) with $Bv = 450$ from previous change
coin:

$$ \text{Available} = (20 - 10) * 90 + 100 = 1000 $$
$$ \text{Withdrawn} = 1000 - 450 = 550 $$
$$ \text{WithdrawCoinValue} = 1000 - 550 = 450 $$
$$ \text{VestingChangeValue} = 450 - 450 = 0 $$
$$ \text{Cumulative: } 280 + 270 + 450 = 1000 = T $$

Expanding $\text{VestingChangeValue}$:

$$ \begin{aligned}
\text{VestingChangeValue} = Bv - \text{WithdrawCoinValue} \\
= Bv - (\text{Available} - T + Bv) \\
= T - \text{Available} \\
\end{aligned} $$

at $t = 12$, $change=1000-280=720$

at $t = 15$, $change=1000-550=450$

at $t = 20$, $change=1000-1000=0$

This means 

$$ \text{WithdrawCoinValue} = Bv - (T - \text{Available}) = Bv - \text{VestingChangeValue} $$

which is just the difference between what the coin held and what must
remain locked.

We can compute
$$ \text{VestingChangeValue} = T - \text{Available} $$

then derive
$$ \text{WithdrawCoinValue} = Bv - \text{VestingChangeValue} $$

Proof simplification:
$$ \text{VestingChangeValue} = \text{BaseSub}(T, \text{Available}) $$
$$ \text{WithdrawCoinValue} = \text{BaseSub}(Bv, \text{VestingChangeValue}) $$

## Forfeit

With this call, a vesting authority is able to forfeit a specific
vesting, by removing the vesting configuration, burning the remaining
balance vested coin and mint a new one to a recipient address, using a
chained transfer call.
