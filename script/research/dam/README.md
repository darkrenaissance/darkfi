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
