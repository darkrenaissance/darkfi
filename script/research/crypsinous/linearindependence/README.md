# target function approximation

excluding use of floats, and division, only +,-,* are allowed.

# target function emulation

## target function

- target fuction T: $$ T = L * \phi(\sigma) = L * (1- (1 - f)^{\sigma}) $$
- $\sigma$ is relative stake.
- f is tuning parameter, or the probability of winning have all the stake
- L is field length

## $\phi(\sigma)$ approximation

- $$\phi(\sigma) = 1 - (1-f)^{\sigma} $$
- $$ = 1 - e^{\sigma ln(1-f)} $$
- $$ = 1 - (1 + \sum_{n=1}^{\infty}\frac{(\sigma ln (1-f))^n}{n!}) $$
- $$ \sigma = \frac{s}{\Sigma} $$
- s is stake, and $\Sigma$ is total stake.

## target T n term approximation

- $$ k = L ln (1-f)^1 $$
- $$ k^{'n} =  L ln (1-f)^n $$
- $$ T = -[k\sigma + \frac{k^{''}}{2!} \sigma^2 + \dots +\frac{ k^{'n}}{n!}\sigma^n] $$
- $$  = -[\frac{k}{\Sigma}s + \frac{k^{''}}{\Sigma^2 2!} s^2 + \dots +\frac{k^{'n}}{\Sigma^n n!} s^n] $$

# comparison of original target to approximation

![approximation comparison to orignal](https://github.com/darkrenaissance/darkfi/blob/master/script/research/crypsinous/linearindependence/target.png?raw=true)

# consequences

- hard coded tunning.
- public reward function.


# conclusion

as the derivative of deltas graph shows, starting for term 2, the derivatives is ~ 0, and it's the optimal number of terms in approximation accuracy that has the least number of terms.
