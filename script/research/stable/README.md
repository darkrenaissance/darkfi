# Darkfi collateral-backed stablecoin (Nun[^3])

Collateral backed stablecoin with low volatility redemption price (based-off Dai) over darkfi blockchain requires:

- Governance DAO for managing safes/vaults parameters.
- Order book exchange of DAO governance token (Nut[^4]) with Nun [anonymously](../bulletproof-mpc/README.md).
- [Blind debt/surplus auction](https://medium.com/@vaheandonians/publicly-verifiable-sealed-bid-auctions-with-a-trustless-auctioneer-4aa50197f00c) for selling, and buying Nun, and Nut.
- Price oracles.

## Governance Dao

In order for the Nun governing Dao community to vote on proposals using [Darkfi dao](https://darkrenaissance.github.io/darkfi/spec/dao/index.html), it's needed to commit to protocol proposal statements hash, or collateral vault contract call data, otherwise passed proposals could be faked.

## Price oracle

Although price oracle can be challenging in anonymous exchange, renegade[^1] dark pool reveal price midpoint pair at match phase, and can be used as price oracle.


## variable redemption price.

Pegging redemption price to 1 dollar is good for the time being for storing value in a low volatility coin as long as the dollar is holding value while can't be maintained for any fiat currency at time or erosion of dollar hegemony, alternatively redemption price can be set by feedback loop similar to Rai[^2] can help Nun hold value during times of high inflation of fiat currency.

[^1]: https://www.renegade.fi/whitepaper.pdf
[^2]: https://raw.githubusercontent.com/reflexer-labs/whitepapers/master/English/rai-english.pdf
[^3]: deep darkness of cosmic ocean.
[^4]: governing sky
