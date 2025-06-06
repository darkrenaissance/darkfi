# DEX

With cross-chain bridging and darkpool DEX then DarkFi would be a very
attractive trading tool.

We currently have OTC swaps which are an efficient way to settle trades
on-chain. But there needs to be an orderbook for matching
counterparties.

We propose that this job can be performed by an order-matching bot
which maintains an orderbook. Later this role can be split with MPC.

The order-matching party needs to be able to construct the swap trade,
even when the LP is offline.

We propose to do this using the spend hook and a special `auth_otc`
function. The `auth_otc` basically says:

* The sender of the funds has the right at any time to withdraw
  liquidity and cancel the LP.
* The funds are delegated to the order-matching party but with
  restrictions:
    * Can only make an OTC swap tx using the funds.
    * The trade parameters are completely specified such as the price
      and currencies.

The funds are therefore delegated to the order-matching party, who
finds counterparty offers and executes the trades settling them
on-chain. Both parties receive their funds in a trustless anonymous
manner. At any time the LP can be canceled and funds withdrawn.
