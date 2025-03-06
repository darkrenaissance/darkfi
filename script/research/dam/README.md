dam
=======

Denial-of-service Analysis Multitool.<br>
This is a suite of tools to simulate flooding attacks on a
P2P network, to verify and fine tune protection mechanisms
against them.<br>
A daemon, a command-line client and a localnet script are
provided.

## damd

Dummy daemon implementing some P2P communication protocols,
along with JSON-RPC endpoints to simulate flooding attacks
over the network.

## dam-cli

Command-line client for `damd`, to trigger flooding attacks
and monitor responses.

## dam-localnet

Localnet folder with script and configuration to deploy
instances to test with.

## Flood testing

Here is a table of flooding scenarios to perfor to verify expected
behavior, based on configured messages parameters.<br>

| # | Description                           | Configuration  | Outcome                                                                                |
|---|---------------------------------------|----------------|----------------------------------------------------------------------------------------|
| 0 | No metering                           | Default        | All flood messages get propagated instantly                                            |
| 1 | Same metering everywhere              | (0,1,6,500,10) | All flood messages eventually get propagated following rate limit rules                |
| 2 | `node0` metering, `node1` no metering | (0,1,6,500,10) | node0 disconnects/bans node1 for flooding                                              |
| 3 | `node0` no metering, `node1` metering | (0,1,6,500,10) | All flood messages eventually get propagated following rate limit rules                |
| 4 | Only `Bar` metered                    | (0,1,6,500,10) | `Foo` messages get propagated instantly while `Bar` messages eventually get propagated |


### Methodology note

Message configuration tuple legend:

| Pos | Description                             |
|-----|-----------------------------------------|
| 0   | MAX_BYTES                               |
| 1   | METERING_SCORE                          |
| 2   | MeteringConfiguration.threshold         |
| 3   | MeteringConfiguration.sleep_step (ms)   |
| 4   | MeteringConfiguration.expiry_time (sec) |

When different configurations are used between the two nodes, you
have to manually compile `damd` with the corresponding message
configuration, copy/move/rename the binary and update the localnet
script accordingly.<br>
Each message can be configured in their corresponding protocol file.<br>
All paths are relative from this folder.

| Message       | Path                                     |
|---------------|------------------------------------------|
| `Bar`         | `damd/src/proto/protocol_bar.rs::L49-55` |
| `FooRequest`  | `damd/src/proto/protocol_foo.rs::L49-55` |
| `FooResponse` | `damd/src/proto/protocol_foo.rs::L64-70` |
