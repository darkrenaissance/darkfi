Architecture design
===================

This section of the book shows the software architecture of DarkFi and
the network implementations.

For this phase of development we organize into teams lead by a single
surgeon. The role of the team is to give full support to the surgeon
and make his work effortless and smooth.

| Component   | Description                                                            | Surgeon | Copilot | Assistant | Status   |
|-------------|------------------------------------------------------------------------|---------|---------|-----------|----------|
| consensus   | Algorithm for blockchain consensus                                     | err     | agg     | das       | Progress |
| zk / crypto | ZK compiler and crypto algos                                           | par     | nar     |           | Mature   |
| wasm        | WASM smart contract system                                             | par     | nar     | xsan      | Progress |
| net         | p2p network protocol code                                              | agg     | xsan    | nar       | Mature   |
| blockchain  | consensus + net + db                                                   | err     | das     |           | Easy     |
| bridge      | Develop robust & secure multi-chain bridge architecture                | par     | xsan    |           | None     |
| tokenomics  | Research and define DRK tokenomics                                     | xeno    | err     | nar       | Starting |
| util        | Various utilities and tooling                                          | nar     | xsan    | das       | Progress |
| arch        | Architecture, project management and integration                       | nar     |         |           | Progress |

Priorities:

1. consensus
2. wasm
3. util
