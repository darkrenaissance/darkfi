# Summary

# About

- [DarkFi](README.md)
- [Philosophy](philosophy/philosophy.md)
  - [Ideology](philosophy/ideology.md)
  - [Books](philosophy/books.md)

# User Guide

- [Running a Node](testnet/node.md)
- [Airdrops](testnet/airdrop.md)
- [Payments](testnet/payment.md)
- [Atomic Swap](testnet/atomic-swap.md)
- [DAO](testnet/dao.md)
- [ircd](misc/ircd/ircd.md)
	- [Private Message](misc/ircd/private_message.md)
	- [Local Deployment](misc/ircd/local_deploy.md)

# Developer Doc

- [Development](dev/dev.md)
  - [Contribute](dev/contrib/contrib.md)
    - [Using Tor](dev/contrib/tor.md)
  - [Learn](dev/learn.md)
  - [API Rustdoc](dev/rustdoc.md)
  - [Native Contracts](dev/native_contracts.md)
  - [Seminars](dev/seminars.md)
- [Architecture](arch/arch.md)
  - [Overview](arch/overview.md)
  - [Anonymous assets](arch/anonymous_assets.md)
  - [Blockchain](arch/blockchain.md)
  - [Consensus](arch/consensus.md)
    - [GenesisStake](arch/consensus/genesis_stake.md)
    - [Stake](arch/consensus/stake.md)
    - [Proposal](arch/consensus/proposal.md)
    - [UnstakeRequest](arch/consensus/unstake_request.md)
    - [Unstake](arch/consensus/unstake.md)
  - [Transactions](arch/tx_lifetime.md)
  - [Smart Contracts](arch/smart_contracts.md)
  - [Bridge](arch/bridge.md)
  - [Tooling](arch/tooling.md)
  - [P2P Network](arch/p2p-network.md)
  - [Services](arch/services.md)
  - [Smart Contracts](arch/sc/sc.md)
    - [Transaction lifetime](arch/sc/tx-lifetime.md)
  - [DAO](arch/dao.md)
- [zkas](zkas/index.md)
  - [Bincode](zkas/bincode.md)
  - [zkVM](zkas/zkvm.md)
  - [Examples](zkas/examples.md)
    - [Anonymous voting](zkas/examples/voting.md)
    - [Anonymous payments](zkas/examples/sapling.md)
- [Client](clients/clients.md)
  - [darkfid JSON-RPC API](clients/darkfid_jsonrpc.md)
  - [faucetd JSON-RPC API](clients/faucetd_jsonrpc.md)
  - [Anonymous Nodes](clients/anonymous_nodes.md)
    - [Tor Inbound Node](clients/tor_inbound.md)
    - [Nym Outbound Node](clients/nym_outbound.md)

# Crypto

- [FFT](crypto/fft.md)
- [ZK explainer](crypto/zk_explainer.md)
- [Research](crypto/research.md)
- [Rate-Limit Nullifiers](crypto/rln.md)
- [Key Recovery Scheme](crypto/key-recovery.md)
- [Reading maths books](crypto/reading-maths-books.md)

# DEP

- [DEP 0001: Version Message Info](dep/0001.md)
- [DEP 0002: Smart Contract Composability](dep/0002.md)

# Specs

- [Notation](spec/notation.md)
- [Concepts](spec/concepts.md)
- [Cryptographic Schemes](spec/crypto-schemes.md)
- [Contracts]()
  - [DAO](spec/contracts/dao/dao.md)
    - [Concepts](spec/contracts/dao/concepts.md)
    - [Model](spec/contracts/dao/model.md)
    - [Contract](spec/contracts/dao/contract.md)
  - [Money](spec/contracts/money.md)

# P2P API Tutorial

- [P2P API Tutorial](learn/dchat/dchat.md)
 - [Deployment](learn/dchat/deployment/part-1.md)
   - [Getting started](learn/dchat/deployment/getting-started.md)
   - [Writing a daemon](learn/dchat/deployment/writing-a-daemon.md)
   - [Sessions](learn/dchat/deployment/sessions.md)
   - [Settings](learn/dchat/deployment/settings.md)
   - [Start-Run-Stop](learn/dchat/deployment/start-stop.md)
   - [Seed](learn/dchat/deployment/seed-node.md)
   - [Deploy](learn/dchat/deployment/deploy.md)
 - [Creating dchatd](learn/dchat/creating-dchatd/part-2.md)
   - [Message](learn/dchat/creating-dchatd/message.md)
   - [Understanding Protocols](learn/dchat/creating-dchatd/protocols.md)
   - [ProtocolDchat](learn/dchat/creating-dchatd/protocol-dchat.md)
   - [Register protocol](learn/dchat/creating-dchatd/register-protocol.md)
   - [Sending messages](learn/dchat/creating-dchatd/sending-messages.md)
   - [Accept addr](learn/dchat/creating-dchatd/accept-addr.md)
   - [Handling RPC requests](learn/dchat/creating-dchatd/rpc-requests.md)
   - [StoppableTask](learn/dchat/creating-dchatd/stoppable-task.md)
   - [Adding methods](learn/dchat/creating-dchatd/rpc-methods.md)
 - [Creating dchat-cli](learn/dchat/creating-dchat-cli/part-3.md)
   - [UI](learn/dchat/creating-dchat-cli/ui.md)
   - [Using dchat](learn/dchat/creating-dchat-cli/using-dchat.md)
 - [Net tools](learn/dchat/network-tools/part-4.md)
   - [get_info](learn/dchat/network-tools/get-info.md)
   - [Attaching dchat](learn/dchat/network-tools/attaching-dnet.md)
   - [Using dnet](learn/dchat/network-tools/using-dnet.md)

# Misc

- [tor-darkirc](misc/tor-darkirc.md)
- [vanityaddr](misc/vanityaddr.md)
- [IRCd Specification](misc/ircd/specification.md)
- [tau](misc/tau.md)
- [event_graph](misc/event_graph/event_graph.md)
  - [Network Protocol](misc/event_graph/network_protocol.md)
- [dnetview](misc/dnetview.md)
- [Zero2darkfi](zero2darkfi/zero2darkfi.md)
  - [darkmap](zero2darkfi/darkmap.md)
- [Glossary](glossary/glossary.md)
