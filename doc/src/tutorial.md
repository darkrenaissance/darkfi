# DarkFi User Tutorial

Welcome to the dark renaissance. This tutorial will teach you how to
install darkfi on your system, and how to use the demo to send and
receive anonymous tokens.

## Download

To run darkfi, we must first install the software. Do this by cloning
the darkfi repo:

```
% git clone https://github.com/darkrenaissance/darkfi
```

## Build

In the project root directory, run provided Makefile. This will download
the trusted setup params and compile the source code. This might take
a while.

```
% make
```

## Install

We will now install the project. This will install the binaries
and configurations in the configured namespace (`/usr/local`
by default). The configurations are installed as TOML files in
`/usr/local/share/doc/darkfi`. They have to be copied in your user's
`HOME/.config/darkfi` directory.

Feel free to review the installed config files, but you don't need to
change anything to run the demo. The defaults will work fine.

```
% sudo make install
% mkdir -p ~/.config/darkfi
% cp -f /usr/local/share/doc/darkfi/*.toml ~/.config/darkfi
```

## Run

Darkfi consists of several software daemons or processes. These daemons
have separate, isolated concerns.

As a user, your interest is in the `darkfid` daemon. This is a user
node that interacts with your wallet and communicates with services on
the darkfi network. It is operated using the `drk` command-line tool.

After the installation, you should have `drk` and `darkfid` binaries in
`/usr/local`. Also, the params and configuration files should be in
`~/.config/darkfi`.

We're now ready to use the demo.

Open two terminal windows. In one terminal, start `darkfid`:

```
% darkfid -v
```

And another terminal, run `drk`. This is the command-line interface to
interact with `darkfid`.

```
% drk -h
drk

USAGE:
    drk [FLAGS] [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    Increase verbosity

OPTIONS:
    -c, --config <CONFIG>    Sets a custom config file

SUBCOMMANDS:
    deposit     Deposit clear tokens for Dark tokens
    features    Show what features the cashier supports
    hello       Say hello to the RPC
    help        Prints this message or the help of the given subcommand(s)
    id          Get hexadecimal ID for token symbol
    transfer    Transfer Dark tokens to address
    wallet      Wallet operations
    withdraw    Withdraw Dark tokens for clear tokens
```

## Deposit

We'll go through the main features one by one. Let's start by depositing
some coins into darkfi.

First, we need testnet coins on either Bitcoin or Solana. For
Bitcoin these can be acquired from a faucet like [this
one](https://testnet-faucet.mempool.co/). You will need to switch your
Bitcoin wallet to testnet mode.

If you don't have a Bitcoin wallet, [Electrum](https://electrum.org/)
is an excellent option. Configuring Electrum
for testnet depends on the operating system: see [this
tutorial](https://bitzuma.com/posts/a-beginners-guide-to-the-electrum-bitcoin-wallet/#testnet-on-ubuntu)
for more details.

For Solana, you can either install the Solana command-line suite or use
[sollet](https://www.sollet.io).

Follow [this tutorial](https://docs.solana.com/cli) for the Solana
command-line. For sollet.io, switch the network to testnet and click
'Request Airdrop' to airdrop yourself some testnet coins.

Now that we have testnet coins we can deposit into darkfi.

We'll do this by sending testnet coins to the darkfi cashier, which will
issue darkened versions of the deposited coin. This process of darkening
involves the cashier minting new anonymous tokens that are 1:1 redeemable
for deposits. For example, if you deposit 1 BTC, you will receive 1 dBTC,
or darkened SOL.

To deposit testnet BTC:

```
% drk deposit btc --network bitcoin
```

To deposit testnet SOL:

```
% drk deposit sol --network solana
```

To deposit any other asset:

```
% drk deposit [ASSET] --network solana
```

This command will send a deposit request to the cashier. After running
it, you should get an address printed to your terminal, like this:

```
Deposit your coins to the following address: "734JBp3FRPoDs6ibSMyq3CV9zyMiDeynxMQRbSxVvozN"
```

Using Bitcoin or Solana, deposit the desired tokens to the specified
address. Wait a moment- it should take about 30 seconds to receive your
deposit. You can follow the progress on the terminal window where you ran
`darkfid`. Then check your updated balance, like so:

```
% drk wallet --balances

+-------+--------+---------+
| token | amount | network |
+-------+--------+---------+
| SOL   | 1      | solana  |
+-------+--------+---------+

```

## Send

Now that you have darkened tokens inside darkfi, you can send them
around anonymously.

Find a friend with an account on darkfi and ask them for their darkfi
address. Then run the transfer command:

```
% drk transfer <TOKEN> <ADDRESS> <AMOUNT>
```

For example, to transfer 1 SOL to a user at
9GmLk7kkbxhsbLTYFMeg6FyuQJV9Na2GcJYFNrs3VLkv address, you would run the
following command:

```
% drk transfer sol 9GmLk7kkbxhsbLTYFMeg6FyuQJV9Na2GcJYFNrs3VLkv 1
```

## Receive

To receive anonymous tokens on your darkfid account, you must retrieve your
darkfi address. Send this address to others so they can send you tokens.

```
% drk wallet --address
Wallet address: "9GmLk7kkbxhsbLTYFMeg6FyuQJV9Na2GcJYFNrs3VLkv"
```

## Withdraw

Withdrawing your testnet funds can be done at any time. This will exchange
your anonymous darkened tokens for their underlying collateral, i.e. if
you have 1 dBTC you will receive 1 BTC.

To do so, simply send the following request to the cashier:

```
% drk withdraw <TOKENSYM> <ADDRESS> <AMOUNT> --network <network>
```

For example, if you want to withdraw 0.5 SOL to the Solana address
4q7rkNvH5BVs6VLz6nyhKLvqXmwDyjGnUsE5sZzRNgp4, you would write:

```
% drk withdraw sol 4q7rkNvH5BVs6VLz6nyhKLvqXmwDyjGnUsE5sZzRNgp4 0.5 --network solana
```

For Bitcoin, the command would look like this:

```
% drk withdraw btc bc1qw7nt2yca0zykh8a5sc6nmy3r3clx4ha206wepn 0.5 --network bitcoin
```

## Configure

DarkFi is highly configurable by design. Key system parameters can be
changed inside the config files.

This is the darkfid config file. Your local copy can be found in
`~/.config/darkfi`.

```toml
## darkfid configuration file
##
## Please make sure you go through all the settings so you can configure
## your daemon properly.

# The address where darkfid should bind its RPC socket
rpc_listen_address = "127.0.0.1:8000"

# Whether to listen with TLS or plain TCP
serve_tls = false

# Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
# This can be created using openssl:
# openssl pkcs12 -export -out identity.pfx -inkey key.pem -in cert.pem -certfile chain_certs.pem
tls_identity_path = "~/.config/darkfi/darkfid_identity.pfx"

# Password for the created TLS identity. (Unused if serve_tls=false)
tls_identity_password = "FOOBAR"

# The endpoint to a gatewayd protocol API
gateway_protocol_url = "tcp://185.165.171.77:3333"

# The endpoint to a gatewayd publisher API
gateway_publisher_url = "tcp://185.165.171.77:4444"

# Path to mint.params
mint_params_path = "~/.config/darkfi/mint.params"

# Path to spend.params
spend_params_path = "~/.config/darkfi/spend.params"

# Path to the client database
database_path = "~/.config/darkfi/darkfid_client.db"

# Path to the wallet database
wallet_path = "~/.config/darkfi/darkfid_wallet.db"

# The wallet password
wallet_password = "TEST_PASSWORD"

# The configured cashiers to use.
[[cashiers]]

# Cashier name
name = "testnet.cashier.dark.fi"

# The RPC endpoint for a selected cashier
#rpc_url = "tcp://127.0.0.1:9000"
rpc_url = "tcp://185.165.171.77:9000"

# The selected cashier public key
public_key = "2MezH7FrtzGwtEEeTU8anM2b67Nzfv8XsojggGUavCUd"
```  

