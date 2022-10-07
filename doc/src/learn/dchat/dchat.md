# Dchat: Writing a p2p app

This tutorial will teach you how to deploy an app on DarkFi's p2p network.

We will create a terminal-based p2p chat app called dchat that we run
in two different instances: an inbound and outbound node called Alice
and Bob. Alice takes a message from `stdin` and broadcasts it to the
p2p network. When Bob receives the message on the p2p network it is
displayed in his terminal.

Dchat will showcase some key concepts that you'll need to develop on
the p2p network, in particular:

* Understanding inbound, outbound and seed nodes.
* Writing and registering a custom `Protocol`.
* Creating and subscribing to a custom `Message` type.

The source code for this tutorial can be found at
[example/dchat](https://github.com/darkrenaissance/darkfi/tree/master/example/dchat).

