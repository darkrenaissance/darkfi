# Overview

DarkFi is a layer one proof-of-stake blockchain that supports anonymous
applications. It is currently under development. This overview will
outline a few key terms that help explain DarkFi.

**Cashier:** The Cashier is the entry-point to the DarkFi network. Its
role is to exchange cryptocurrency assets for anonymous _darkened_ tokens
that are pegged to the underlying currency. Currently, the role of the
Cashier is trusted and centralized. As a next step, DarkFi plans to
implement trust-minimized bridges and eventually fully trustless bridges.

**Blockchain:** Once new anonymous tokens (e.g. dETH) have been issued,
the Cashier posts that data on the blockchain. This data is encrypted
and the transaction link is broken.

The DarkFi blockchain is currently using a very simple consensus protocol
called Streamlet. The blockchain is currently in devnet phase. This is a
local testnet ran by the DarkFi community. Currently, the blockchain has
no consensus token. DarkFi is working to upgrade to a privacy-enhanced
proof-of-stake algorithm called OuroborusCrypsinous.

**Wallets:** A wallet is a portal to the DarkFi network. It provides
the user with the ability to send and receive anonymous _darkened_
tokens. Each wallet is a full node and stores a copy of the
blockchain. All contract execution is done locally on the DarkFi wallet.

**P2P Network:** The DarkFi ecosystem runs as a network of P2P nodes,
where these nodes interact with each other over specific protocols (see
[node overview](dna.md)). Nodes communicate on a peer-to-peer network,
which is also home to tools such as our P2P [irc](../misc/ircd.md)
and P2P task manager [tau](../misc/tau.md).

**zkas:** zkas is the compiler used to compile zk smart contracts in
its respective assembly-like language. The "assembly" part was chosen as
it's the bare primitives needed for zk proofs, so later on the language
can be expanded with higher-level syntax. Its underlying zero-knowledge
proof system is Halo2.

**ZK contracts:** Anonymous applications on DarkFi run on proofs
that enforce an order of operations. We call these zero-knowledge
contracts. Anonymous transactions on DarkFi is possible due to the
interplay of two contracts, mint and burn (see the [sapling payment
scheme](../zkas/examples/sapling.md)). Using the same method, we can
define advanced applications.
