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

Let $t₀ = \t{BlockWindow} ∈ 𝔽ₚ$ be the current blockwindow as defined
in [Blockwindow](model.md#blockwindow).

Let $Bv ∈ ℕ₆₄$ be the burned coin.

The core formula to compute amounts corresponding to the current block
window is:

$$ \begin{aligned}
CurrentBlockwindow = CondSelect(BlockwindowCond, t₀, E); \\
BlockwindowsPassed = CurrentBlockwindow - S; \\
Available = (BlockwindowsPassed * V) + C; \\
Withdrawn = T - Bv; \\
WithdrawCoinValue = Available - Withdrawn; \\
VestingChangeValue = T - (Withdrawn + WithdrawCoinValue);
\end{aligned} $$

The vesting schedule model says that any blockwindow $t$ where
$S <= t <= E$, the total amount that should have been unlocked is:

$$ \begin{aligned}
Available(t) = (t - S) * V + C;
\end{aligned} $$

And we know from the vest proof's constraint that $T = (E-S) * V + C$,
so $Available(E) = T$. The schedule is linear between $S$ and $E$ with
a cliff C at the start.

The burned vested coin has value $Bv$ which represents the remaining
balance in the vested coin. Initially (right after vest) $Bv = T$.
After each withdrawal it shrinks.

So "total withdrawn so far" is $T - Bv$ and the formula computes how
much new value the vestee can take:

$$ \begin{aligned}
WithdrawCoinValue = Available - (T - Bv) = Available - T + Bv; \\
VestingChangeValue = Bv - WithdrawCoinValue;
\end{aligned} $$

Concrete example:

Let:

$$ \begin{aligned}
T = 1000; \\
C = 100; \\
S = 10; \\
E = 20; \\
V = 90; \\
(20 - 10) * 90 + 100 = 1000;
\end{aligned} $$

First withdrawal at $t = 12$ with $Bv = 100$ as the initial vested coin:

$$ \begin{aligned}
Available = (12 - 10) * 90 + 100 = 280; \\
Withdrawn = 1000 - 1000 = 0; \\
WithdrawCoinValue = 280 - 0 = 280; \\
VestingChangeValue = 1000 - 280 = 720;
\end{aligned} $$

Conservation: $280 + 720 = 1000 = Bv$

Second withdrawal at $t = 15$ with $Bv = 720$ from previous change coin:

$$ \begin{aligned}
Available = (15 - 10) * 90 + 100 = 550; \\
Withdrawn = 1000 - 720 = 280; \\
WithdrawCoinValue = 550 - 280 = 270; \\
VestingChangeValue = 720 - 270 = 450;
\end{aligned} $$

Conservation: $270 + 450 = 720 = Bv$

Cumulative withdrawn: $280 + 270 = 550 = Available(15)$

Final withdrawal at $t = 20$ (end) with $Bv = 450$ from previous change
coin:

$$ \begin{aligned}
Available = (20 - 10) * 90 + 100 = 1000; \\
Withdrawn = 1000 - 450 = 550; \\
WithdrawCoinValue = 1000 - 550 = 450; \\
VestingChangeValue = 450 - 450 = 0;
\end{aligned} $$

Cumulative: $280 + 270 + 450 = 1000 = T$

Expanding $VestingChangeValue$:

$$ \begin{aligned}
VestingChangeValue = Bv - WithdrawCoinValue; \\
VestingChangeValue = Bv - (Available - T + Bv); \\
VestingChangeValue = T - Available;
\end{aligned} $$

at $t = 12$, $change=1000-280=720$

at $t = 15$, $change=1000-550=450$

at $t = 20$, $change=1000-1000=0$

^ This means $WithdrawCoinValue = Bv - (T - Available) = Bv -
VestingChangeValue$ which is just the difference between what the coin
held and what must remain locked.

We can compute $VestingChangeValue = T - Available$ then derive
$WithdrawCoinValue = Bv - VestingChangeValue$.

Proof simplification:
$$ \begin{aligned}
VestingChangeValue = BaseSub(T, Available); \\
WithdrawCoinValue = BaseSub(Bv, VestingChangeValue)
\end{aligned} $$

## Forfeit

With this call, a vesting authority is able to forfeit a specific
vesting, by removing the vesting configuration, burning the remaining
balance vested coin and mint a new one to a recipient address, using a
chained transfer call.
