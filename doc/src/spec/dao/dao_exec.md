# dao execution


$$ X = (bulla, coin^{in}, coin^{out}, cm^{vote^{yes}}_x, cm^{vote^{yes}}_y, cm^{vote^{all}}_x, cm^{vote^{all}}_y, cm^{value^{in}}_x, cm^{value^{in}}_y, spendHook^{dao}, spendHook^{user}, data, \dots) $$

- (TODO) why dao exec contract spend hook doesn't have data? although it's public input.


# circuit checks

- $quorum <= vote^{all}$
- $vote^{all} *
