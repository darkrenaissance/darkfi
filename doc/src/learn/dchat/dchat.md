# Dchat: Writing a p2p app

This tutorial will teach you how to deploy an app on DarkFi's p2p network.

We will create a terminal-based p2p chat app called dchat. The chat app
has two parts: a p2p daemon called `dchatd` and a python command-line
tool for interacting with the daemon called `dchat-cli`.

Dchat will showcase some key concepts that you'll need to develop on
the p2p network, in particular:

* Creating a p2p daemon.
* Understanding inbound, outbound and seed nodes.
* Writing and registering a custom `Protocol`.
* Creating and subscribing to a custom `Message` type.

The source code for this tutorial can be found at
[example/dchat](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/example/dchat).
