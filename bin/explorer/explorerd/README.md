# Explorerd

The `explorerd` is a Rust-based daemon responsible for running a DarkFi Explorer node to synchronize blockchain data with DarkFi nodes across multiple networks. During startup, it syncs missing or outdated blocks and then listens for live updates to ensure the Explorer provides accurate and up-to-date information about blockchain activity, including blocks, transactions, and key metrics such as gas consumption.

The `explorerd` is designed to handle real-world blockchain events, such as reorganizations ("reorgs"), missing blocks, and divergences between networks. Its primary purpose is to ensure a consistent view of DarkFi blockchain data across different networks. For its storage layer, it leverages **sled** to take advantage of its performance, scalability, and reliability.

---

## Key Features

- **Full Blockchain Synchronization**: Ensures the Explorer's database reflects the current state of the DarkFi blockchain by syncing missing or outdated block data.
- **Reorg Mitigation**: Detects and resolves chain reorganizations to maintain alignment with the respective networks.
- **Real-Time Updates**: Leverages DarkFi's subscription interface to receive live block and transaction data.
- **On-The-Fly Metric Calculations**: Computes analytics and blockchain metrics for use in the Explorer's UI. This includes maintaining data such as running totals, min/max values, and transaction counts when processing blocks, allowing for efficient gas metric calculations without iterating through previous transactions.

---

## Getting Started

### Prerequisites
Before you begin, ensure you have the following installed and configured:

- **Rust 1.86.0 or later**: For building and running the explorer daemon (`explorerd`).
- **Darkfid**: Installed and configurable using [Darkfid Config](../../darkfid/darkfid_config.toml).
- **Minerd**: Installed and configurable using [Minerd Config](../../minerd/minerd_config.toml).

---

### Configuration

The `explorerd` uses a TOML configuration file to manage different environment settings, located at [Explorerd Config](explorerd_config.toml). If no configuration file is provided, a default configuration file is automatically generated at `~/.config/darkfi/explorerd_config.toml` the first time the `explorerd` daemon is run. This pre-configured `explorerd` configuration is set up to connect to the DarkFi `darkfid` localnet, testnet, and mainnet networks.

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

---

### Supported DarkFi Blockchain Networks

The `explorerd` daemon is pre-configured to support the following DarkFi blockchain environments:

- **`localnet`**: Ideal for development and testing in a local environment using `darkfid`'s localnet configuration.
- **`testnet`**: Used for testing the explorer with `darkfid`'s testnet network (pending availability).
- **`mainnet`**: The production environment for running the explorer on DarkFi's canonical chain (pending availability).

These blockchain environments are defined in the `darkfid` configuration file, located at [Darkfid Config](../darkfid/darkfid_config.toml). The pre-configured `explorerd` network settings are configured to connect to the `darkfid` nodes running these networks.

> **Note**: Once the testnet and mainnet are fully available, the explorer will sync more than just the genesis blocks for these networks.

### Installation

Navigate to the `explorerd` directory, build the binary using `make`, and install it:

```bash
cd bin/explorer/explorerd
make install
```

> Use `make` without `install` to build the `explorerd` binary in `bin/explorerd` without installing it.

---

### Running the Daemon

#### 1. Configure the Daemons

- **For Darkfid Localnet**: Explorerd is pre-configured to connect to the respective `darkfid` networks. No additional configuration is required when running `explorerd` without a configuration file.
- **For Custom Configurations**: To create a custom configuration file, use [explorerd_config.toml](../explorerd_config.toml) as a template and modify it as needed. Ensure that the `endpoint` in the `explorerd` configuration matches the `rpc_listen` value in the corresponding `darkfid_config.toml` file, enabling proper connection to the corresponding DarkFi blockchain node.

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

- **For Contrib/Localnet Networks**: If you're using a `contrib/localnet` network, update the `endpoint` in your `explorerd_config.toml` to match the `rpc_listen` endpoint in the respective darkfid configuration.

> For information on configuring `darkfid` and `minerd`, refer to their respective documentation.

#### 2. Start Supporting Daemons

##### Running Against `darkfid's localnet`

Start the `minerd` and `darkfid` daemons using their default configurations.

```bash
# Start the mining daemon using default settings
minerd

# Start darkfid using default settings on localnet 
darkfid --network localnet
```

##### Running Against `contrib/localnet/darkfid-single-node`

Run the DarkFi single-node.

```bash
# Change directory and start contrib/localnet/singlenode
cd contrib/localnet/darkfid-single-node
./tmux_sessions.sh
```

#### 3. Start the `explorerd` Daemon
##### Syncing with `darkfid's localnet`

Run the `explorerd` daemon to sync with `darkfid's` localnet blockchain data.

```bash
# Start the explorer daemon using pre-configured localnet configuration
explorerd --network localnet
```

##### Syncing with Custom DarkFi Blockchain Endpoint

Run the `explorerd` daemon using a custom configuration with updated `darkfid` endpoint:

```bash
# Run explorerd using a custom configuration
explorerd -c explorerd-config.toml --network localnet
```

> Ensure the `endpoint` in `explorerd-config.toml` is updated to match the desired `darkfid` endpoint.

---
