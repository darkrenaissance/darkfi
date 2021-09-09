# First time running the demo:

1. Configure gatewayd, cashierd, darkfid and drk TOML files. Copy paste the following defaults to .config/darkfi:

**gatewayd.toml**

```
connect_url = "127.0.0.1:3333"
publisher_url = "127.0.0.1:4444"
database_path = "gatewayd.db"
log_path = "/tmp/gatewayd.log"
```

**cashierd.toml**

```
accept_url = "127.0.0.1:7777"
rpc_url = "http://127.0.0.1:8000"
client_database_path = "cashier_client_database.db"
btc_endpoint = "tcp://electrum.blockstream.info:50001"
gateway_url = "127.0.0.1:3333"
log_path = "/tmp/cashierd.log"
cashierdb_path = "/home/x/.config/darkfi/cashier.db"
client_walletdb_path = "/home/x/.config/darkfi/cashier_client_walletdb.db"
password = ""
client_password = ""
```

**darkfid.toml**

```
connect_url = "127.0.0.1:3333"
subscriber_url = "127.0.0.1:4444"
cashier_url = "127.0.0.1:7777"
rpc_url = "127.0.0.1:8000"
database_path = "/home/x/.config/darkfi/database_client.db"
walletdb_path = "/home/x/.config/darkfi/walletdb.db"
log_path = "/tmp/darkfid_service_daemon.log"
password = ""
```

**drk.toml**

```
rpc_url = "http://127.0.0.1:8000"
log_path = "/tmp/drk_cli.log"
```

2. Configure the password field on all TOML files.

3. Compile the project:

```console
$ cargo build --release
```

4. Run the gateway daemon:

```console
$ cargo run --bin gatewayd -- -v
```

5. Run cashierd:

```console
$ cargo run --bin cashierd -- -v
```

6. Run darkfid:

```console
$ cargo run --bin darkfid -- -v
```

7. Initialize drk wallet and generate a key pair:

```console
$ cargo run --bin drk -- -wk 
```

8. Play.

```console
$ cargo run --bin drk -- -help
```

# Every time running the demo:

Run gateway daemon:

```console
$ cargo run --bin gatewayd -- -v
```

Run cashierd:

```console
$ cargo run --bin cashierd -- -v
```

Run darkfid:

```console
$ cargo run --bin darkfid -- -v
```

Show drk usage manual:

```console
$ cargo run --bin drk -- -help
```

# darkfid & drk configurations:

Darkfid and drk can be configured using the TOML files in the .config/darkfid directory. Make sure to recompile darkfid and drk after customizing the TOML.

# Go dark

Let's liberate people from the claws of big tech and create the democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
