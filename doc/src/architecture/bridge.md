Anonymous Bridge (DRAFT)
========================

We present an overview of a possibility to develop anonymous bridges
from any blockchain network that has tokens/balances on some address
owned by a secret key. Usually in networks, we have a secret key which
we use to derive a public key (address) and use this address to receive
funds. In this overview, we'll go through such an operation on the
Ethereum network and see how we can bridge funds from ETH to DarkFi.

## Preliminaries

**Verifiable secret sharing**[^1]

Verifiable secret sharing ensures that even if the dealer is malicious
there is a well-defined secret that the players can later reconstruct.
VSS is defined as a secure multi-party protocol for computing the
randomized functionality corresponding to some secret sharing scheme.


**Secure multiparty computation**[^2]

Multiparty computation is typically accomplished by making secret
shares of the inputs, and manipulating the shares to compute some
function. To handle "active" adversaries (that is, adversaries that
corrupt nodes and make them deviate from the protocol), the secret
sharing scheme needs to be verifiable to prevent the deviating nodes
from throwing off the protocol.


## General bridge flow

Assume Alice wants to bridge 10 ETH from the Ethereum network into
DarkFi. Alice would issue a bridging request and perform a VSS scheme
with a network of nodes in order to create an Ethereum secret key,
and with it - derive an Ethereum address. Using such a scheme should
prevent any single party to retrieve the secret key and steal funds.
This also means, for every bridging operation, a fresh and unused
Ethereum address is generated and as such gives no convenient ways
of tracing bridge deposits.

Once the new address has been generated, Alice can now send funds
to the address and either create some proof of deposit, or there can
be an oracle that verifies the state on Ethereum in order to confirm
that the funds have actually been sent.

Once confirmed, the bridging smart contract is able to freshly mint
the counterpart of the deposited funds on a DarkFi address of Alice's
choice.

### Open questions:

* **What to do with the deposited funds?**

It is possible to send them to some pool or smart contract on ETH,
but this becomes an address that can be blacklisted as adversaries can
assume it is the bridge's funds. Alternatively, it could be sent into
an L2 such as Aztec in order to anonymise the funds, but (for now)
this also limits the variety of tokens that can be bridged (ETH & DAI).

* **How to handle network fees?**

In the case where the token being bridged cannot be used to pay network
fees (e.g. bridging DAI from ETH), there needs to be a way to cover
the transaction costs. The bridge nodes could fund this themselves
but then there also needs to be some protection mechanism to avoid
people being able to drain those wallets from their ETH.

[^1]: <https://en.wikipedia.org/wiki/Verifiable_secret_sharing>

[^2]: <https://en.wikipedia.org/wiki/Secure_multiparty_computation>
