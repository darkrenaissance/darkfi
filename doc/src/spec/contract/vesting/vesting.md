# Vesting

```
status: draft
```

## Abstract

This contract implements fully anonymous vesting, in which all the
vesting information is private. Anyone can become a vesting authority,
submitting coins to-be-vested for another user(or a DAO), the vestee.
After some time has passed, the vestee can withdraw a chunk of the
vested coin value. The vesting authority is also able to forfeit a
vesting at any time, retrieving the remaining vested coin balance.

- [Concepts](concepts.md)
- [Model](model.md)
- [Scheme](scheme.md)

> Open questions:
> 1. Do we need a separate cliff time? If its set thats the start time
> so no real need to keep them both we can assume start == cliff.
> 2. Is using the shared key for signatures safe and needed?
> 3. Should vesting configurations be grouped by authority so is easier
> UX to manage them?
> 4. Is the vested coin encryption verification formula correct?
> 5. Do we need to check both coins in withdraw transfer in the proof or
> its fine since transfer itself enforces them?
> 6. Vesting requires 1-1 vested coin to config matching, which means
> vested coin is trackable as they are used during the vesting process.
> Does that break any anonymity properties? Withdrawed coins cannot be
> tracked, just the vested coin.
> 7. We need to figure out a way to handle withdrawls after end
> blockwindow has passed. We can use `cond_select` where both prover
> and verifier pass the condition checl `current >= end` and in the
> proof we pick current blockwindow or end blockwindow based on that.
> But this require the verifier to know the end blockwindow, unless we
> find a way the condition check can be done without revealing it.
> Another option is to have an explicit `WithdrawAfterEnd` to withdraw
> remaining balance after end blockwindow has passed. We already have
> the metadata leak of ending tracking assumption, so perhaps its
> worthy to sacrifice it.
> 8. Withdrawl calcs correctness? They can also be simplified further
> for proof optimization.
> 9. All calls use the same parameters. Unless we need something in any
> of them they will be the same structure in the final code.
> 10. Do we need to check both coins in forfeit transfer in the proof or
> its fine since transfer itself enforces them?
