ircd-three-peers
================

A local simulation of an ircd network with the topology of three peers.

Connections:

* `A <--> B`
* `A <--> C`

Used to confirm that messages sent **from** `C` end up at `B`, by being
relayed through peer `A`.
