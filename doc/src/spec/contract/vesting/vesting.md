# Vesting

## Abstract

We want to create a fully anonymous vesting contract, in which all the vesting information is private.
What we need, is a vesting authority, which initially submits coins to be vested, along with the vesting
configuration. When this happens, all input coins, which must be for the same token, get burned, and a new
coin is minted for the vested public key, which uses the spend hook of the vesting contract, effectivelly
becoming usable only withing the vesting contract context. After some time has passed, the vestee can
withdraw some coins, which is done by burning the existing vested coin, and creates two output coins;
one representing the remaining balance using the spend hook of the vesting contract and a second one
which will be a normal coin for a withdrawal address(DAOs can be recipients too).

The vesting authority must also be able to forfeit the configured vesting. There is a limitation though;
the vested coin can only be controlled by a single address. To overcome this, when the vesting authority
creates the vested coin, instead of using the recipients actual address, it generates a new random one,
which is shared with the vestee(in a safe off-chain manner), so the coin can be controlled by both parties.
When the vestee withdraws some funds, the new remaining balance token must use the same shared secret key.
We don't care if that becomes a zero value coin, since it won't be usable anymore, and we want all our
outputs to look the same, so the final withdrawl cannot be tracked.

## Vesting configuration

The vesting contract uses 1 day block windows as its time measurement, similar to the DAO contract.

The configuration structure contains the following:
    1. auth_public_x: The vesting autority public key X coord
    2. auth_public_y: The vesting autority public key Y coord
    3. shared_public_x: The shared vestee public key X coord
    4. shared_public_y: The shared vestee public key Y coord
    5. cliff: Amount unlocked at the cliff timestamp.
    6. cliff_window: Vesting contract cliff block window.
    7. start_window: Block window when the tokens start vesting.
    8. end_window: Block window when all the tokens are fully vested.

The above information get hashed by poseidon to produce the VestingBulla, which is used as the on-chain
identifier of this specific configuration.

## Contract calls

TODO: describe all the checks for each call

### Vest
Vesting authority submits a vesting configuration on-chain, burns the to-be-vested input coins and mints
a new coin with their total amount for the shared secret address, using the contracts' spend-hook.

### Withdraw
This call is responsible to define the available to withdraw amount, along with checking the first output
correctly represents the shared secret and uses contract spend-hook. In this call, input coin value
commitment must match the addition of the output coin value commitments. Also we check the next call is a
normal money transfer call, which executes the actual burn and mint functions.

### ExecWithdraw
This is an overlay function, acting as the parent of a Withdraw and money Transfer calls combination, to
bound them together into a single atomic action, and define the input spendhook that must be used in the
transfer call.

### Forfeit
With this call, the vest autority is able to forfeit a specific vesting, by removing the configuration,
burning the existing vest coin and mint a new one to a recipient address, using a normal money transfer.
Input and output coin values must be identical.

### ExecForfeit
This is an overlay function, acting as the parent of a Forfeit and money Transfer calls combination, to
bound them together into a single atomic action, and define the input spendhook that must be used in the
transfer call.

