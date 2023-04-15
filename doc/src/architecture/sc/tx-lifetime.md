Transaction lifetime
====================

Let $T$ be a transaction on the DarkFi network. Each transaction
consists of multiple ordered contract calls:

$$ T = [C_1, …, C_n] $$

Associate with each contract call an operator $fC =$
**contract_function**.  Each contract consists of arbitrary data which
is interpreted by the contract. So for example sending money to a
person, the transaction $T = [C_1]$ has a single call with $fC_1 =$
`Money::Transfer`. To enforce a transaction fee, we can add another
call to `Money::Fee` and now our transaction $T$ would have two calls:
$[C_1, C_2]$.

To move money from a DAO's treasury, we can build a transaction
$T = [C_1, C_2, C_3]$ where:

* $fC_1 =$ `Money::Fee`
* $fC_2 =$ `Money::Transfer`
* $fC_3 =$ `DAO::Exec`

This illustrates the concept of chaining function calls together in
a single transaction.

## `Money::Transfer`

Denote the call data here simply by $C$. Since payments on DarkFi use
the Sapling UTXO model, there are $n$ inputs $I_i$ and $m$ outputs
$O_j$ in $C$. There are also $\pi_n$ input _burn_ zero-knowledge
proofs, and $\mu_m$ output _mint_ zero-knowledge proofs.

Each input $I_i$ contains a nullifier $N_i$ which is deterministically
generated from the previous output's (the output which is being spent)
serial code $\rho_i$ and secret key $x_i$. The ZK burn proof $\pi_i$
states:

1. Correct construction of the nullifier $N_i = \textrm{hash}(x_i, ρ_i)$,
   revealing this value publicly.
2. Derive the public key $P_i = x_iG$.
3. Construct the coin commitment $C_i = \textrm{hash}(x_i, v_i, \tau_i, \rho_i, …, b_i)$,
   where $v_i$ is the coin value, $\tau_i$ is the token ID, and $b_i$
   is a random blinding factor. Additional metadata may be stored in
   this coin commitment for additional functionality.
4. Set membership proof that $C_i \in R$ where $R$ represents the set
   of all presently existing coins.
5. Any additional checks such as value and token commitments.

Outputs $O_j$ contain the publiic coin commitment $C_j$, a proof of
their construction $\mu_j$, and corresponding value/token commitments.
The unlinkability property comes from only the nullifier $N$ being
revealed in inputs (while $C$ is hidden), while the coin $C$ appears
in outputs (but without nullifiers). Since there is a deterministic
derivation of nullifiers from $C$, you cannot double spend coins.

The ZK mint proof is simpler and consists of proving the correct
construction of $C$ and the corresponding value/token commitments.

To hide amounts, both proofs export value commitments on the coin
amounts. They use a commitment function with a homomorphic property:

$$ \phi : \mathbb{F} \rightarrow E $$
$$ \phi(x + y) = \phi(x) + \phi(y) $$

So to check value is preserved across inputs and outputs, it's merely
sufficient to check:
$$ \sum_{u_i \in U} \phi(u_i) = \sum_{v_j \in V} \phi(v_j) $$

## `DAO::Exec`

Earlier we mentioned that bullas/coins can contain arbitrary metadata
(indicated by …). This allows us to construct the concept of protocol
owned liquidity. Inside the coin we can store metadata that is checked
for correctness by subsequent contract calls within the same transaction.
Take for example $T = [C_1, C_2]$ mentioned earlier. We have:

* $fC_1 =$ `Money::Transfer`
* $fC_2 =$ `DAO::Exec`

Now the contract $C_2$ will use the encrypted DAO value exported
from $C_1$ in its ZK proof when attempting to debit money from the
DAO treasury. This enables secure separation of contracts and also
enables composability in the anonymous smart contract context.

The DAO proof states:

1. There is a valud active proposal $P$, and
   $P = \textrm{hash}(Q, v, …, D)$, where $(Q, v)$ are the destination
   public key and amount, and $D$ is the DAO commitment.
2. That $D = \textrm{hash}(q, r, …)$ where $q$ is the quorum threshold
   that must be met (minimum voting activity) and $r$ is the required
   approval ratio for votes to pass (e.g. 0.6).
3. Correct construction of the output coins for $C_1$ which are sending
   money from the DAO treasury to $(Q, v)$ are specified by the
   proposal, and returning the change back to the DAO's treasury.
4. Total sum of votes meet the required thresholds $q$ and $r$ as
   specified by the DAO.

By sending money to the DAO's treasury, you add metadata into the coin
which when spent requires additional contract calls to be present in
the transaction $T$. These additional calls then enforce additional
restrictions on the structure and data of $T$ such as is specified
above.
