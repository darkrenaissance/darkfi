ircd-five-peers
===============

A local simulation of an ircd network with the topology of five peers.

Connections:

* `A <--> B`
* `A <--> C`
* `A <--> D`
* `B <--> E`


## Ports

|Node|JSON-RPC|  P2P  |  IRC  |
|----|--------|-------|-------|
| A  | 25550  | 25560 | 25570 |
| B  | 25551  | 25561 | 25571 |
| C  | 25552  | 25562 | 25572 |
| D  | 25553  | 25563 | 25573 |
| E  | 25554  | 25564 | 25574 |
