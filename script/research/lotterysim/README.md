---
title: darkfi lottery simulation
author: ertosns
date: 11/1/2023
---

simulate darkfi consensus lottery with a discrete controller

# discrete pid controller.
control lottery f tunning paramter

$$f[k] = f[k-1] + K_1e[k] + K_2e[k-1] + K_3e[k-2]$$

with $k_1 = k_p + K_i + K_d$,  $k_2 = -K_p -2K_d$,  $k_3 = K_d$, and e is the error function.

# simulation criterion
find $K_p$, $k_i$, $K_d$ for highest accuracy running the simulation on N trials, of random number of nodes, starting with random airdrop (that all sum to total network stake), running for random runing time.

![alt text](https://github.com/darkrenaissance/darkfi/blob/master/script/research/lotterysim/img/heuristics.png?raw=true)

notice that best parameters are spread out in the search space, picking the highest of which, and running the simulation, running for 600 slots, result in with >36% accuracy

![alt text](https://github.com/darkrenaissance/darkfi/blob/master/script/research/lotterysim/img/f_history_processed.png?raw=true)

# comparing range of target values between

notice below that both y,T in the pallas field, and simulation have same range.

![alt text](https://github.com/darkrenaissance/darkfi/blob/master/script/research/lotterysim/img/lottery_dist.png?raw=true)

# conclusion

using discrete controller the lottery accuracy > 33% with randomized number of nodes, and randomized relative stake.
can be coupled with khonsu[^1] to achieve 100% accuracy and instant finality.

# usage

Replace `example.csv` with local distribution data. Edit config.py as follows:

```python
vesting_file = 'your_local_data.csv'
```

Edit config.py to define the exchange rate and simulation running time,
measured in slots.

Then run the program:

```shell
python vesting.py
```

[^1]: https://github.com/ertosns/thunderbolt
