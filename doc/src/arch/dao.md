# DAO Short Explainer

## Prerequisites

There is a scheme called commit-and-reveal, where given an object $x$
(or set of objects), you generate a random blind $b$, and construct the
commitment $C = \textrm{hash}(x, b)$. By publishing $C$, you are
commmitted to the value $x$.

Secondly, we may wish to publish long lived objects on the blockchain,
which are essentially commitments to several parameters that represent
an object defining its behaviour. In lieu of a better term, we call
these bullas.

## `DAO::mint()`: Establishing the DAO

From `darkfi/src/contract/dao/proof/dao-mint.zk`:
```
	bulla = poseidon_hash(
		dao_proposer_limit,
		dao_quorum,
		dao_approval_ratio_quot,
		dao_approval_ratio_base,
		gov_token_id,
		dao_public_x,
		dao_public_y,
		dao_bulla_blind,
	);
```

Brief description of the DAO bulla params:

* **proposer_limit**: minimum deposit required for proposals to become valid.
  TODO: rename to `min_deposit`.
* **quorum**: minimum threshold of votes before it's allowed to pass.
  Normally this is implemented as min % of voting power, but we do this in
  absolute value
* **approval_ratio**: proportion of winners to losers for a proposal to pass.

Currently there is no notion of veto although it could be trivially added
if desired.

## `DAO::propose()`: Propose the Vote

From `darkfi/src/contract/dao/proof/dao-propose-main.zk`:
```
	proposal_bulla = poseidon_hash(
		proposal_dest_x,
		proposal_dest_y,
		proposal_amount,
		proposal_token_id,
		dao_bulla,
		proposal_blind,
	);
```

We create a proposal which will send tokens to the dest provided.
This will soon be changed to be generic. Proposals will commit to
calling params or code instead.

## `DAO::vote()`: Vote on a Proposal

Governance token holders each make an encrypted homomorphic commitment to
their vote. The homomorphism is additive so $f(u) + f(v) = f(u + v)$.
They also encrypt their vote to the DAO pubkey.

Finally once voting is completed, the holders of the DAO pubkey (which
is up to DAO policy) can decrypt the votes $f(v₁), …, f(vₙ)$, sum the values
$v₁ + ⋯ + vₙ$ and so have the value which can be used in ZK proofs alongside
the publicly available commitment $f(v₁ + ⋯ + vₙ) = f(v₁) + ⋯ + f(vₙ)$.

## `DAO::exec()`: Execute Passed Proposal

This is the key part. We produce a tx which has two contract calls:
`[money::transfer(), DAO::exec()]`. The coins spent in `money::transfer()`
belong to the DAO and have the condition that they can only be spent
when combined with `DAO::exec()`. Here is what coins in `money::transfer()`
look like:

```
	C = poseidon_hash(
		pub_x,
		pub_y,
		value,
		token,
		serial,
		spend_hook,
		user_data,
	);
```

When we send coins to the DAO treasury, we set `spend_hook` to the DAO contract,
and `user_data` to the DAO bulla.

When spending the coins, they reveal the `spend_hook` publicly and `user_data`
(encrypted). `money::transfer()` enforces that the next contract call must be
the same as the `spend_hook`.

The contract invoked by `spend_hook` can then use the `user_data`. We use this
to store the DAO bulla. `DAO::exec()` will then use this as our DAO, and check
the proposal we are executing belongs to this DAO through the reference to
the DAO bulla in the proposal params.

`DAO::exec()` then encodes the rules that specify there has to be a valid
proposal where voting passed the threshold and so on.

Assuming both contracts validate successfully, the funds are transferred out
of the DAO treasury.

