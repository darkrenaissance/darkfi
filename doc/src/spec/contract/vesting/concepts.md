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

## Forfeit

With this call, a vesting authority is able to forfeit a specific
vesting, by removing the vesting configuration, burning the remaining
balance vested coin and mint a new one to a recipient address, using a
chained transfer call.
