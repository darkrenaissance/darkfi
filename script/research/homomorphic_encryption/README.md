# critical study of darkfi homomorphic encryption,  burn, mint proof

## pouring mechanism

for each transaction a new coin is created, or to spend a coin a new coin is created for the receipient,

old coin $c^{old} = (a_{pk}^{old}, v^{old}, \rho^{old}, r^{old}, s^{old}. cm^{old})$ to be spent  by pouring into $c_1^{new}$, $c_2^{new}$. $c_1^{new}$ can be the value transfered, and $c_2^{new}$ can be the exchange. note that a proof must be given for $c^{old}+c^{pub} = c_1^{new} + c_2^{new}$ and $c^{pub}$ is public coin value, from public network, and it's value is set to zero otherwise: $c^{old} = c_1^{new} + c_2^{new}$.

in this case the stakeholder of the old coin $c^{old}$ can't trace $c_1^{new}$ since the serial number isn't known, and deriven from nonce, and coin secret key, $$sn = RFP^{sn}_{sk^{COIN}}(\rho)$$, thus the scheme is anonoymous, and the stakeholder of the old coin $c^{old}$ can't double spend $c_1^{new}$ since it have no access to the secret key for which the newly created coin is commited to it's corresponding public key.

the pour transaction $tx_{pour} = (rt,sn^{old},cm_1^{new},cm_2^{new},\pi_{POUR})$.
note the pour transaction commit to two-step coin commitment.
$$k = COMMIT_r(a_{pk}||\rho)$$
$$cm_i = COMMIT_s(v||k)$$




### transaction pouring proof (TXFER in crypsinous)

for the circuit inputs, and witnesses

\begin{math}
x = (\{cm_{c_3},cm_{c_4}\},\{sn_{c_1},sn_{c_2}\},\tau,root) \\
w = (root_{sk_{c_1}^{COIN}},path_{sk_{c_1}^{COIN}},root_{sk_{c_2}^{COIN}},path_{sk_{c_2}^{COIN}},pk_{c_3}^{COIN},pk_{c_4}^{COIN},(\rho_{c_1},r_{c_1},v_1,path_1),(\rho_{c_2},r_{c_2},v_2,path_2),
\\(\rho_{c_1},r_{c_1},v_1,path_1))
\end{math}

$\pi$ is a proof for the following transfer statement using zerocash pouring mechanism.

$$\forall_i \in \{1,2\}: pk_{c_i}^{COIN} = PRF_{root_{sk_{c_i}}^{COIN}}^{pk}(1)$$
$$\forall_i \in \{1,\dots,4\} : DeComm(cm_{c_i},pk_{c_i}^{COIN}||v_i||\rho_{c_i},r_{c_i})=T$$
$$v_1+v_2=v_3+v_4$$

$$path_1\text{ is a valid path to } cm_{c_1} \text{ in a tree with the root} \emph{ root}$$

$$path_2\text{ is a valid path to } cm_{c_2} \text{ in a tree with the root} \emph{ root}, sn_{c_2}=PRF_{root_{sk_{c_1}^{COIN}}}^{zdrv}(\rho_{c_1})$$

$$path_{sk_{c_i}^{COIN}} \text{ is a valid path to a leaf at position } \tau \text{ in }, root_{sk_{c_i}^{COIN}} i \in \{1,2\}$$

$$sn_{c_i}=PRF_{root_{sk_{c_i}^{COIN}}}^{sn}(\rho_{c_i}), \forall_i \in \{1,2\}$$

## homomorphic encryption mechanism

spending the coin by nullifier defined as a poseidon hash of the secret key of the sender, and the serial number generated at random as such $H = PRF^{poseidon}(sk||sn)$ as a proof of burn.

and the tx include encrypted note with the receipient public key
``` rust
pub struct Note {
    pub serial: DrkSerial,
    pub value: u64,
    pub token_id: DrkTokenId,
    pub coin_blind: DrkCoinBlind,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
}
```

### homomorphic encryption dosn't solve double spending

alice create tx with $coin^{old}$ with serial number $sn^{old}$, and create transaction output with new coin new serial number $sn^{new}$ choosen at random.
with burn proof that include a nullifier: $$H=PRF^{poseidon}(sk_{alice}||sn)$$
and coin commitment published to merkle tree:
$$cm=PRF^{poseidon}(bob_{pk}||v||id|sn_{old}||r)$$

before bob can spend his $coin^{new}$ alice can double spend this coin simply since alice knows bob public key being a public value, and v, id, and the new serial number. and can also give a mint proof of bob's coin since alice have access to bob's public key, value, token id, new serial number.
 for spending alice would calculate  nullifier $$H`=RPF^{poseidon}(sk_{alice}||sn^{new})$$
 validator will find that H`!=H, and it will pass the validation, secondly alice can give mint proof of the bob's coin  as such:
 $$cm=PRF^{poseidon}(bob_pk||v||id|sn_{new}||r)$$

 now alice spent bob's coin simply because bob's coin serial number is known to alice (thus non-anonymous), and secondly because spending the coin is done by sn known to the adversary, and secret key that is non-binding, the adversary can claim the coin and use it's own secret key for double spending.

 finally making nullifier as poseidon of the concatenation sk||sn, and without using key pairs for coins, a rainbow table attack with the combination sk||sn against published nullifier would undermine the security, and anonymity of transactions, a solution to this is using blinding values in the nullifiers, and not using either the serial number or the secret key, as it's done int he pouring mechanism.
