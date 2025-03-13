# Explorerd

The `explorerd` is a Rust-based daemon responsible for running a DarkFi Explorer node to synchronize blockchain data with DarkFi nodes across multiple networks. During startup, it syncs missing or outdated blocks and then listens for live updates to ensure the Explorer provides accurate and up-to-date information about blockchain activity, including blocks, transactions, and key metrics such as gas consumption.

The `explorerd` is designed to handle real-world blockchain events, such as reorganizations ("reorgs"), missing blocks, and divergences between networks. Its primary purpose is to ensure a consistent view of DarkFi blockchain data across different networks. For its storage layer, it leverages **sled** to take advantage of its performance, scalability, and reliability.

---

## Key Features

- **Full Blockchain Synchronization**: Ensures the Explorer's database reflects the current state of the DarkFi blockchain by syncing missing or outdated block data.
- **Reorg Mitigation**: Detects and resolves chain reorganizations to maintain alignment with the respective networks.
- **Real-Time Updates**: Leverages DarkFi's subscription interface to receive live block and transaction data.
- **On-The-Fly Metric Calculations**: Computes analytics and blockchain metrics for use in the Explorer's UI. This includes maintaining data such as running totals, min/max values, and transaction counts when processing blocks, allowing for efficient gas metric calculations without iterating through previous transactions.

## Network Status

The testnet and mainnet configurations serve as placeholders in preparation for their respective launch. When starting daemons with these configurations, each node will connect to their respective darkfid network, but currently only sync the network's genesis block. Once the DarkFi blockchain testnet and mainnet are fully available, the explorer daemon will sync blocks beyond genesis for these networks.

## Prerequisites

- **Rust 1.86.0 or later**: For building and running the explorer daemon (`explorerd`).
- **Darkfi Project Dependencies**: Dependencies required to compile the Darkfi code. For more details, see [Darkfi Build Dependencies](../../../README.md#build).
- **Darkfid**: Required for running DarkFi blockchain nodes on respective networks. The make commands build the binary from source code in `../../darkfid` (if not already built in project root) and apply the appropriate network configuration.
- **Minerd**: Needed for setups where Darkfid is configured with a miner JSON-RPC endpoint, but the configured miner is not running on the desired network. The make commands build the binary from source code in `../../minerd` (if not already built in project root) and apply the appropriate network configuration.

## Quick-Start Guide

Run the following commands to run a node on respective network.

### Start a Localnet Node

```sh
# Run a node using localnet configuration 
make start-localnet
```

### Start a Testnet Node

```sh
# Run a node using testnet configuration 
make start-testnet
```

### Start a Mainnet Node

```sh
# Run a node using mainnet configuration 
make start-mainnet
```

### Stopping the Node

```sh
# Stop the running explorer node
make stop
```

### Confirming Successful Start 

When a DarkFi Explorer Node successfully starts, users should a startup banner displaying the node's configuration details and current sync status. Here is a successful localnet node startup example:

```
03:31:37 [INFO] ========================================================================================
03:31:37 [INFO]                    Started DarkFi Explorer Node                                        
03:31:37 [INFO] ========================================================================================
03:31:37 [INFO]   - Network: localnet
03:31:37 [INFO]   - JSON-RPC Endpoint: tcp://127.0.0.1:14567
03:31:37 [INFO]   - Database: ~/.local/share/darkfi/explorerd/localnet
03:31:37 [INFO]   - Configuration: ./explorerd_config.toml
03:31:37 [INFO]   - Reset Blocks: No
03:31:37 [INFO] ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
03:31:37 [INFO]   - Synced Blocks: 8
03:31:37 [INFO]   - Synced Transactions: 8
03:31:37 [INFO]   - Connected Darkfi Node: tcp://127.0.0.1:8240
03:31:37 [INFO] ========================================================================================
```

### Getting Help
```shell
make help
```

## Detailed Guide

### Configuration

The `explorerd` uses a TOML configuration file to manage different network settings, located at [Explorerd Config](explorerd_config.toml). These settings are configured to automatically connect to DarkFi blockchain nodes running on localnet, testnet, and mainnet. When running an explorer daemon for the first time without a configuration file, the default configuration file is automatically copied to `~/.config/darkfi/explorerd_config.toml`. Once this file exists, running explorerd again will automatically start a node based on this configuration.

#### Example Configuration

Below is an example of a localnet configuration for `explorerd` (`~/.config/darkfi/explorerd_config.toml`):

```toml
#~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
# Localnet Configuration
#~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[network_config."localnet"]

# Path to daemon database
database = "~/.local/share/darkfi/explorerd/localnet"

# darkfid JSON-RPC endpoint
endpoint = "tcp://127.0.0.1:8240"

## Localnet JSON-RPC settings
[network_config."localnet".rpc]
# JSON-RPC listen URL
rpc_listen = "tcp://127.0.0.1:14567"

# Disabled RPC methods
#rpc_disabled_methods = []
```

**Note**: When updating the configuration, ensure the `endpoint` field matches the `rpc_listen` endpoint in the respective `darkfid` configuration, and that the `database` and `rpc_listen` paths are consistent with your environment.

#### Supported DarkFi Blockchain Networks

The explorer daemon comes pre-configured to automatically connect to the following DarkFi's blockchain environments:
- **`localnet`**: For development and testing in a local environment
- **`testnet`**: For testing on DarkFi's test network (pending availability)
- **`mainnet`**: For production use on DarkFi's main network (pending availability)

For more details about these darkfid networks, see the [Darkfid Configuration](../../darkfid/darkfid_config.toml).

> **Note**: Once the testnet and mainnet are fully available, the explorer will sync more than just the genesis blocks for these networks.

#### Custom Configuration

To create a custom explorerd configuration, use [Explorerd Configuration](../explorerd_config.toml) as a template and modify it as needed. Ensure that the `endpoint` in the `explorerd` configuration matches the `rpc_listen` value in the corresponding `darkfid_config.toml` file, enabling proper connection to the corresponding DarkFi blockchain node.

**Example Alignment**:

- `darkfid_config.toml`:

  ```toml
  [network_config."localnet".rpc]
  rpc_listen = "tcp://127.0.0.1:8240"
  ```

- `explorerd_config.toml`:

  ```toml
  [network_config."localnet"]
  endpoint = "tcp://127.0.0.1:8240"
  ```

### Installation

The `explorerd` binary can be installed to your system path using the `make install` command, which places it in `~/.cargo/bin/explorerd`. This directory is typically included in your PATH, making the command accessible system-wide.

From the `explorerd` directory:

```bash
make install
```

> Use `make` without `install` to build the `explorerd` binary in `bin/explorerd` without installing it.

### Running an Explorer Node

#### 1. Start Supporting Nodes

##### Using darkfid's networks

Start the `minerd` and `darkfid` daemons using their configurations.

```bash
# Start the mining daemon
minerd

# Start darkfid (example uses localnet) 
darkfid --network localnet
```

> Replace `localnet` with `testnet` or `mainnet` to connect to those networks instead.

##### Using the single-node development environment

Run the DarkFi single-node development environment.

```bash
# Change directory and start the single-node environment
cd contrib/localnet/darkfid-single-node
./tmux_sessions.sh
```
> Ensure you update your explorerd configuration file to point to the single node network

#### 2. Start the `explorerd` Daemon

##### Connecting to darkfid networks

Run the `explorerd` daemon to sync with the darkfid instance.

```bash
# Start the explorer daemon (example uses localnet)
explorerd --network localnet
```

> Replace `localnet` with `testnet` or `mainnet` to connect to those networks instead.

##### Connecting to custom networks

To connect to a custom blockchain network, provide a configuration file with your specific network settings:

```bash
# Run explorerd with a custom configuration
explorerd -c explorerd-config.toml --network localnet
```

> Update the `endpoint` in your `explorerd-config.toml` to point to your custom `darkfid` instance. The `--network` parameter can be set to `localnet`, `testnet`, or `mainnet` to match your target network configuration.