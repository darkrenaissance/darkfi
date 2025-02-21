dam localnet
================

This will start two `damd` node instances in localnet mode.
The first node is considered the defender, and we will listen
to its incoming messages, while the second one is the attacker,
so we will listen to its outgoing messages.
Second node can be queried to start attacking the other one,
using `dam-cli`.
