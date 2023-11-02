# DAO execution

DAO execution proof once quorum, and yes vote required have been met.

$$ X = (bulla, coin^{proposal}, coin^{dao}, cm^{vote^{yes}}_x, cm^{vote^{yes}}_y, cm^{vote^{all}}_x, cm^{vote^{all}}_y, cm^{value^{proposal}}_x, cm^{value^{prososal}}_y, spendHook^{dao}, spendHook^{proposal}, data^{proposal}) $$

$$ W = (proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, proposal^{tokenId}, blind^{proposal}, proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, pub_x, pub_y, blind^{bulla}, vote^{yes}, vote^{all}, blind^{vote^{yes}}, blind^{vote^{all}}, sn^{proposal}, sn^{dao}, value^{dao}, blind^{value^{dao}}, spendHook^{dao}, spendHook^{proposal}, data^{proposal}) $$

$$ \mathcal{L}= \{X:(X,W)\in \mathcal{R}\} $$

## Execution proof.

- Derive, and reveal [bulla](bulla.md)
- Derive, and reveal [proposal](proposal.md) input [coin](../payment/coin.md)
- Calculate, and  Reveal yes-vote 0/1 for no/yes [commitment](../crypto/commitment.md) $=vote^{yes}vote^{all^}$
- Reveal all-vote value [commitment](../crypto/commitment.md)
- Calculate, and reveal [proposal](proposal.md) coin value [commitment](../crypto/commitment.md)
- Reveal dao execution spendHook
- Reveal proposal spendHook
- Reveal proposal spendHook input data
- Reveal dao spendHook input data: [bulla](bulla.md) [commitment](../crypto/commitment.md)

| Public inputs              | Description                                                          |
|----------------------------|----------------------------------------------------------------------|
| bulla                      | dao [bulla](bulla.md)                                                |
| $coin^{proposal}$          | [proposal](proposal.md) input [coin](../payment/coin.md)             |
| $coin^{dao}$               | dao [coin](../payment/coin.md)                                       |
| $cm^{vote^{yes}}_x$        |  yes vote [commitment](../crypto/commitment.md) x coordinate          |
| $cm^{vote^{yes}}_y$        | yes vote [commitment](../crypto/commitment.md) y coordinate           |
| $cm^{vote^{all}}_x$        | all votes [commitment](../crypto/commitment.md) x coordinate          |
| $cm^{vote^{all}}_y$        | all votes [commitment](../crypto/commitment.md) y coordinate          |
| $cm^{value^{proposal}}_x$  | x-coordinate of [commitment](../crypto/commitment.md) to $value^{proposal}$                      |
| $cm^{value^{proposal}}_y$  | y-coordinate of [commitment](../crypto/commitment.md) to $value^{proposal}$                      |
| $spendHook^{dao}$          | dao spendhook                                                        |
| $spendHook^{proposal}$     | [proposal](proposal.md) spendhook                                                   |
| $data^{proposal}$          | input data for $spendhook^{proposal}$ contract                       |
| $data^{dao}$               | input data for $spendhook^{proposal}$ contract                       |


| Witnesses                  | Destination                                            |
|----------------------------|--------------------------------------------------------|
| $proposal^{destination}_x$ | [proposal](proposal.md) destination [public key](../payment/keypair.md) x coordinate           |
| $proposal^{destination}_y$ | [proposal](proposal.md) destination [public key](../payment/keypair.md) y coordinate           |
| $proposal^{amount}$        | [proposal](proposal.md) amount in proposal token                      |
| $proposal^{tokenId}$       | [proposal](proposal.md) [token id](../payment/token_id.md)            |
| $blind^{proposal}$         | [proposal](proposal.md) [commitment](../crypto/commitment.md) blind factor|
| proposerLimit              | governance token necessary for the vote to be valid                   |
| quorum                     | minimum number of votes necessary to pass the [proposal](proposal.md) |
| $approvalRatio_{quot}$     | [proposal](proposal.md) approval ratio quotient                       |
| $approvalRatio_{base}$     | [proposal](proposal.md) approval ratio base                           |
| tokenId                    | governance [token id](../payment/token_id.md)                         |
| $pub_x$                    | dao [public key](../payment/keypair.md) x coordinate                  |
| $pub_y$                    | dao [public key](../payment/keypair.md) y coordinate                  |
| $blind^{bulla}$            | [bulla](bulla.md) [commitment](../crypto/commitment.md) blinding factor|
| $vote^{yes}$               | yes vote a boolean as either 0, or 1                                  |
| $vote^{all}$               | all votes value                                                       |
| $blind^{vote^{yes}}$       | yes vote [commitment](../crypto/commitment.md) blinding factor        |
| $blind^{vote^{all}}$       | blinding term for all votes [commitment](../crypto/commitment.md)s    |
| $sn^{proposal}$            | serial number for [proposal](proposal.md) [coin](../payment/coin.md)  |
| $sn^{dao}$                 | dao input [coin](../payment/coin.md) serial number                    |
| $value^{dao}$              | dao input [coin](../payment/coin.md) value                            |
| $blind^{value^{dao}}$       | dao [coin](../payment/coin.md)  value blinding term                  |
| $spendHook^{dao}$          | dao spendhook                                                         |
| $spendHook^{proposal}$     | [proposal](proposal.md) spendhook                                                    |
| $data^{proposal}$          | input data for $spendhook^{proposal}$ contract                        |


# Circuit checks

- $ quorum <= vote^{all}$
- $ \frac{approvalRatio^{quot}}{approvalRatio^{base}} <= \frac{vote^{yes}}{vote^{all}} $
