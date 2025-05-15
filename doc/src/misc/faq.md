# Frequently Asked Questions

## What is DarkFi?
DarkFi is an ecosystem of anonymous applications. It consists of a layer 1 
blockchain, a communications service, and a task management service. The
communications service `darkirc` is an anonymous IRC server. `tau` is a task 
management app that gives users the ability to collaborate with others including 
assigning and syncing tasks across different workspaces. DarkFi is built 
with strong privacy, censorship-resistance, and free (as in freedom) and open 
source software philosophy as its guiding design principles.

## How is the DarkFi blockchain different than other privacy claiming projects?
<u><b>Note</b></u>: Each network's design choices and architecture widely varies. 
This is only to point out general design differences between DarkFi and others.

DarkFi is a proof-of-work layer 1 blockchain and ecosystem. DarkFi ZK circuits 
are programmed in ZKAS (ZK Assembly) and then executed in the zkVM to generate 
proofs on-chain. DarkFi uses Halo 2 for its proving system, which requires no 
trusted setup. Since DarkFi is an L1, all transactions are executed directly by 
the network. 

Bitcoin is a transparent blockchain where people execute coin transfers between 
participants. Similarly, Monero is also a blockchain where people transfer coins
between participants, but they do can do it in a private manner. Ethereum is a
transparent blockchain, where people can execute custom smart contracts, as well
as transfer coins. DarkFi aims to achieve a similar concept, but instead of 
transparency, everything is built in a privacy first manner. Let's look at a few 
other similar projects within the ecosystem.

Aztec Network is an Ethereum layer 2 zk-rollup, and uses their own domain specific 
language, Noir. Aztec uses PLONK as its proving system, which requires a trusted 
setup. Transactions on Aztec are added to the rollup block by network sequencers 
to settle on the Ethereum L1. 

Aleo is a proof-of-stake layer 1 blockchain that focuses on building ZK dApps. 
Aleo required a trusted setup for its foundational zk-proofs. Aleo uses their 
domain specific language, Leo. Transactions are submitted to the Aleo network 
via snarkOS.

Namada is a proof-of-stake layer 1 blockchain that aims to build an interchain
platform for shielded transfers of arbitrary assets. Namada's natively built with 
inter-operability for IBC-chains. There is the ability to shield and unshield 
transactions. Namada used a trusted setup to generate the random parameters for 
the MASP circuit (which is an extension of the sapling circuit). Transactions 
propagate directly to the network and verification is done by "accounts" on-chain.

## What type of consensus does DarkFi use?
DarkFi is a proof-of-work layer 1 blockchain, using 
[RandomX](https://github.com/tevador/RandomX). RandomX is optimized for 
general-purpose CPUs, and is also used by Monero. You can find more information
about the DarkFi consensus process [here](../arch/consensus.md).

## How can I chat with DarkFi devs?
Join [DarkIRC](darkirc/darkirc.md), our peer-to-peer anonymous implementation of 
an IRC server. There are weekly `#dev` meetings on Mondays.

## How can I contribute to the project or build something on top of DarkFi?
You can visit [here](../dev/contrib/contrib.md)
and ask any questions related to development in the `#dev` `darkirc` channel. You 
can also familiarize yourself with our docs by 
[starting here](../start-here.md).

## Where should I go if I'm having network connectivity issues with DarkFi?
If you are having trouble connecting DarkFi applications, please refer to 
[network troubleshooting](network-troubleshooting.md).

## How can I run the testnet?
Follow the [testnet guide](../testnet/node.md).

## How can I run my DarkFi nodes over Tor?
You can setup a Tor enabled node [here](nodes/tor-guide.html).
