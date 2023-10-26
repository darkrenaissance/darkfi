# dao execution

$$ X = (bulla, coin^{proposal}, coin^{dao}, cm^{vote^{yes}}_x, cm^{vote^{yes}}_y, cm^{vote^{all}}_x, cm^{vote^{all}}_y, cm^{value^{proposal}}_x, cm^{value^{prososal}}_y, spendHook^{dao}[^1], spendHook^{proposal}, data^{proposal}) $$

$$ W = (proposal^{destination}_x, proposal^{destination}_y, proposal^{amount}, proposal^{tokenId}, blind^{proposal}, proposerLimit, quorum, approvalRatio_{quot}, approvalRatio_{base}, tokenId, pub_x, pub_y, blind^{bulla}, vote^{yes}, vote^{all}, blind^{vote^{yes}}, blind^{vote^{all}}, sn^{proposal}, sn^{dao}, value^{dao}, blind^{value^{dao}}, spendHook^{dao}, spendHook^{proposal}, data^{proposal}) $$

$$ \mathcal{L}= \{X:W\in \mathcal{R}\} $$


| public inputs              | Description                                            |
|----------------------------|--------------------------------------------------------|
| bulla                      | dao bulla                                              |
| $coin^{proposal}$          | proposal coin                                          |
| $coin^{dao}$               | dao coin                                               |
| $cm^{vote^{yes}}_x$        | x-coordinate of commitment to $vote^{yes}$ value       |
| $cm^{vote^{yes}}_y$        | y-coordinate of commitment to $vote^{yes}$ value       |
| $cm^{vote^{all}}_x$        | x-coordinate of commitment to $vote^{all}$ value       |
| $cm^{vote^{all}}_y$        | y-coordinate of commitment to $vote^{all}$ value       |
| $cm^{value^{proposal}}_x$  | x-coordinate of commitment to $value^{proposal}$       |
| $cm^{value^{proposal}}_y$  | y-coordinate of commitment to $value^{proposal}$       |
| $spendHook^{dao}$          | dao spendhook                                          |
| $spendHook^{proposal}$     | proposal spendhook                                     |
| $data^{proposal}$          | input data for $spendhook^{proposal}$ contract         |



| witnesses                  | Destination                                            |
|----------------------------|--------------------------------------------------------|
| $proposal^{destination}_x$ | proposal destination public key x coordinate           |
| $proposal^{destination}_y$ | proposal destination public key y coordinate           |
| $proposal^{amount}$        | proposal amount in proposal token                      |
| $proposal^{tokenId}$       | proposal token id                                      |
| $blind^{proposal}$         | proposal commitment blind factor                       |
| proposerLimit              | governance token necessary for the vote to be valid    |
| quorum                     | minimum number of votes necessary to pass the proposal |
| $approvalRatio_{quot}$     | proposal approval ratio quotient                       |
| $approvalRatio_{base}$     | proposal approval ratio base                           |
| tokenId                    | governance token id                                    |
| $pub_x$                    | dao public key x coordinate                            |
| $pub_y$                    | dao public key y coordinate                            |
| $blind^{bulla}$            | bulla commitment blinding factor                       |
| $vote^{yes}$               | yes vote a boolean as either 0, or 1                   |
| $vote^{all}$               | all votes value                                        |
| $blind^{vote^{yes}}$       | yes vote commitment blinding factor                    |
| $blind^{vote^{all}}$       | blinding term for all votes commitments                |
| $sn^{proposal}$            | serial number for proposal coin                        |
| $sn^{dao}$                 | dao coin serial number                                 |
| $value^{dao}$              | dao coin input value                                   |
| $blind^{value^{dao}}$       | dao coin  value blinding term                         |
| $spendHook^{dao}$          | dao spendhook                                          |
| $spendHook^{proposal}$     | proposal spendhook                                     |
| $data^{proposal}$          | input data for $spendhook^{proposal}$ contract         |


# circuit checks

- $ quorum <= vote^{all}$
- $ \frac{approvalRatio^{quot}}{approvalRatio^{base}} <= \frac{vote^{yes}}{vote^{all}} $

[^1]:  why dao exec contract spend hook doesn't have data? although it's public input.
