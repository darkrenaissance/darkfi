PID controller rewrite spec
================

This document describes the spec planning for the PID controller rewrite,
needed to simplify the current implementation, along with pseudo code
representing each functionality.

# Slot sigmas

We need a function to return 2-term target approximation sigma coefficients,
corresponding to provided slot consensus state, represented as `pallas::Base`.
To generate the slot sigmas, we have to perform the following:

1. Calculate the inverse probability `f` of becoming a block producer (winning the lottery)
   having all the tokens, represented as Float10.
2. Calculate network total tokens, deriving from genesis stake, genesis mint,
   and all block rewarded tokens, represented as Float10.
3. Calculate the sigmas using previous 2 numbers, represented as pallas::Base.

Each step will be further described in the sub-sections following.

Pseudocode:
```
/// Return 2-term target approximation sigma coefficients,
/// corresponding to provided slot consensus state.
fn sigmas() -> (pallas::Base, pallas::Base) {
    let f: Float10 = calculate_f();
    let total_tokens: Float10 = total_tokens();
    calculate_sigmas(f, total_tokens)
}
```

## Calculate f

In this step we execute the actual PID controller calculation to
calculate `f`. This calculation asumes we keep track of historic
data, like the error feedback and the values themselves. To achieve
that, we will store these values in each generated slot, so everyone
can validate them in sequence, as those values are based on each slot
previous values, therefore showcasing the progression up to that point
in time.

Pseudocode:
```
/// Calculate the inverse probability `f` of becoming a block producer (winning the lottery)
/// having all the tokens, represented as Float10.
fn calculate_f() -> Float10 {
    // PID controller K values based on constants
    let k1 = KP + KI + KD;
    let k2 = FLOAT10_NEG_ONE * KP + FLOAT10_NEG_TWO * KD;
    let k3 = KD;
    
    // Calculate feedback error based on previous block producers.
    // We know how many producers existed in previous slot by
    // the len of its fork hashes.
    let feedback: Float10 = previous_slot.fork_hashes.len();
    let err = FLOAT10_ONE - feedback;
    
    // Calculate f
    let f = previous_slot.f + k1 * err + k2 * previous_slot.err + k3 * previous_previous_slot.err;
    
    // Boundaries control
    if f <= FLOAT10_ZERO {
        f = MIN_F.clone()
    } else if f >= FLOAT10_ONE {
        f = MAX_F
    }
    
    f
}
```

## Calculate total tokens

In this step we calculate network total tokens.

Pseudocode:
```
/// Network total tokens, assuming constant reward.
/// Only used for fine-tuning. Since a genesis staker
/// is needed to progress the blockchain, we know that
/// total tokens > 0, as genesis_tokens > 0.
fn total_tokens() -> u64 {
    // Retrieve existing blocks count, excluding genesis,
    // up until current slot
    let blocks = blockchain.len_until(slot) - 1;
    // Calculate rewarded slots, including previous slot
    // longest fork proposals
    let rewarded_slots = blocks + previous_slot.longest_fork_length;
    // Calculate rewarded tokens
    let rewarded_tokens = rewarded_slots * REWARD
    
    rewarded_tokens + genesis_tokens
}
```

## Calculate sigmas

Finally we can produce the slot sigmas, based on previous calculations.

Pseudocode:
```
/// Return 2-term target approximation sigma coefficients,
/// corresponding to provided `f` and `total_tokens` values.
fn calculate_sigmas(f: Float10, total_tokens: Float10) -> (pallas::Base, pallas::Base) {
    // Field `P` value represented as `Float10`
    let field_p: Float10 = P;

    // Calculate `neg_c` value
    let x = FLOAT10_ONE - f;
    let c = x.ln();
    let neg_c = FLOAT10_NEG_ONE * c;

    // Calculate sigma 1
    let sigma1_fbig = neg_c / (total_tokens + FLOAT10_EPSILON) * field_p;
    let sigma1 = fbig2base(sigma1_fbig);

    // Calculate sigma 2
    let sigma2_fbig = (neg_c / (total_tokens + FLOAT10_EPSILON)).powf(FLOAT10_TWO) * (field_p / FLOAT10_TWO);
    let sigma2 = fbig2base(sigma2_fbig);

    (sigma1, sigma2)
}
```
