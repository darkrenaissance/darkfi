# Overview

DarkFi is a layer one Proof-of-Work blockchain that supports anonymous
applications. It is currently under development. This overview will
outline a few key terms that help explain DarkFi.

**Blockchain:** The DarkFi blockchain is based off Proof of Work
RandomX algorithm. Consensus participating nodes, called miners,
produce and propose new blocks to the network, extending some fork
chain, which once it reaches a confirmation security threshold, can be
appended to canonical by all nodes in the network.

**Wallet:** A wallet is a portal to the DarkFi network. It provides
the user with the ability to send and receive anonymous _darkened_
tokens. Each wallet is a full node and stores a copy of the
blockchain. All contract execution is done locally on the DarkFi wallet.

**P2P Network:** The DarkFi ecosystem runs as a network of P2P nodes,
where these nodes interact with each other over specific protocols (see
[node overview](p2p-network.md)). Nodes communicate on a peer-to-peer
network, which is also home to tools such as our P2P
[irc](../misc/darkirc/darkirc.md) and P2P task manager [tau](../misc/tau.md).

**ZK smart contracts:** Anonymous applications on DarkFi run on proofs
that enforce an order of operations. We call these zero-knowledge smart
contracts. Anonymous transactions on DarkFi are possible due to the
interplay of two contracts, mint and burn (see the [sapling payment
scheme](../zkas/examples/sapling.md)). Using the same method, we can
define advanced applications.

**zkas:** zkas is the compiler used to compile zk smart contracts in
its respective assembly-like language. The "assembly" part was chosen as
it's the bare primitives needed for zk proofs, so later on the language
can be expanded with higher-level syntax. Zkas enables developers to
compile and inspect contracts.

**zkVM:** DarkFi's zkVM executes the binaries produced by zkas. The
zkVM aims to be a general-purpose zkSNARK virtual machine that empowers
developers to quickly prototype and debug zk contracts. It uses a
trustless zero-knowledge proof system called Halo 2 with no trusted
setup.
