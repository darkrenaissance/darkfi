# zerocash notes, and highlights

## zerocoin drawbacks
zerocoin isn't used in daily transaction due to performance limitations, but rather for anonymization, or laundrying coins through decentralized mix. performance bottleneck is that redeeming zerocoins requires double-discrete-logarithm proofs of knowledge which have size that exceeds 45kb, and 450ms to verify(at the 128-bit security level), it uses fixed denominations, can't pay in zerocoin directly, provides anonymity for the original address only.

# minting
minting coin $c := ((a_{pk}, PK_{enc}), v, \rho, r, s, cm)$ is:
$$tx_{mint}:=(v,k,s,cm)$$
$$cm :=COMM_{s}(v||k)$$
$$a_{pk} :=RPF_{a_{sk}}^{addr}(0)$$
$$k:=COMM_{r}(a_{pk}||\rho)$$

# spending
Spending the coin c:
$$tx_{spend}:=(cm,sn,\pi)$$
$$sn:=PRF_{a_{sk}}^{sn}(\rho)$$

### pouring
pouring $coin^{old}$ into $coin_1^{new}$, $coin_2^{new}$. with $v^{old}=v_{pub}+v_1^{new}+v_2^{new}$ as follows:
$$tx_{pour}:=(rt,sn^{old},cm_1^{new},cm_2^{new},\pi_{pour}, enc_{pk_{enc,1}^{new}}(C_1),enc_{pk_{enc,2}^{new}}(C_2)$$
$$C_i=(v_i^{new},\rho_i^{new},r_i^{new}, s_i^{new})$$
$\pi_{pour}$ is the pouring sk-snark proof of the spending/pouring process.
address for each participant is the pair $(addr_{pk}, addr_{sk})$, $addr_{pk}=(a_{enc},pk_{enc})$, $addr_{sk}=(a_{sk}, sk_{enc})$
