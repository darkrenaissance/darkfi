# DarkFi Explorer Site

The Explorer Site is a python based web application that serves as the user interface for viewing DarkFi blockchain data. It provides an interactive way to explore blockchain information, gas analytics, and DarkFi native contract source code, connecting with explorer nodes for real-time data.

## Key Features

- **Explorer Node Integration**: Enables real-time blockchain data retrieval, ensuring a consistent and accurate view of blockchain data across supported networks.
- **View Blockchain Data**: View and navigate block and transaction data.
- **Gas Analytics**: Access detailed gas usage metrics across the blockchain and per-transaction basis.
- **Contract Source Code Navigation**: Inspect native contract source code implementations directly.

## Network Status

The testnet and mainnet configurations serve as placeholders in preparation for their respective launch. When starting the site using these environment configurations, the site will connect to an Explorer Node that point to their respective darkfid networks, but currently only display the network's genesis block.

In addition, testnet and mainnet configurations are currently using development servers and work is ongoing for production-like setups.

## Prerequisites
- Python 3.12
- Make
- Explorerd - Explorer node must be installed and running for the network configuration you are running the site against. See [Explorerd README](../explorerd/README.md) for more details.

## Quick-Start Guide

Run the following to get the explorer site running using pre-configured networks that will automatically install dependencies, configure your environment, and start the site server. Please make sure that explorerd is running for the site configuration that you are launching.

### Start Localnet Site

```sh
# Launch site using the localnet explorer node configuration  
make start-localnet
```

### Start Testnet Site

```sh
# Launch site using the latest testnet explorer node configuration 
make start-testnet
```

### Start Mainnet Site

```sh
# Launch site using the latest mainnet explorer node configuration 
make start-mainnet
```

> Once started, navigate to http://127.0.0.1:5000 in your browser to navigate the site.

### Stopping the Site

```sh
# Stop the running explorer site
make stop
```

### Getting Help
```shell
make help
```

## Detailed Guide

### Installation

Install dependencies:
```sh
make install
```

### Configuration

The application uses a TOML configuration file to manage different environment settings located at [Site Config](site_config.toml).

#### Example Configuration

Below is an example configuration for `localnet`.

```toml
[localnet]
# Explorer daemon JSON-RPC endpoint URL
explorer_rpc_url = "127.0.0.1"

# Explorer daemon JSON-RPC port
explorer_rpc_port = 14567

# Path to store log files
log_path = "~/.local/share/darkfi/explorer_site/localnet"
```

#### Pre-Configured Networks

The Explorer Site supports the following pre-configured `explorerd` environments out of the box.
- **`localnet`**: An environment for testing and running `explorerd` locally.
- **`testnet`**: A testing environment for validating the site's functionality prior to mainnet (pending availability).
- **`mainnet`**: The live production environment connected to the canonical DarkFi blockchain network (pending availability).

> Once the DarkFi blockchain testnet and mainnet are fully available, the Explorer Site will show more than just the genesis blocks for these networks.

Each network environment corresponds to a pre-configured explorerd configuration defined in [Explorerd Config](../explorerd/explorerd_config.toml).

#### Custom Configurations

The Explorer Site supports custom configurations to connect to an Explorer Node, whether running locally on the same machine or remotely on a different network. Proper alignment between the Explorer Site and Explorer Node configurations is essential to ensure the site displays blockchain data.

##### Connecting to a Remote Explorer Node

To enable a remote setup, such as for designers testing their UI design, the Explorer Site can be configured to connect to a remote Explorer Node.

To connect to a remote node, configure the Explorer RPC URL and port in the `site_config.toml` file. Ensure that the domain address and port correspond to the remote node's settings.

**Example Configuration for a Testnet**:

`site_config.toml`:

```toml
[testnet]
explorer_rpc_url = "remote-explorer-node.com"
explorer_rpc_port = 80
```

Replace `remote-explorer-node.com` with the domain of your remote node and adjust the port as needed for your specific setup.

##### Connecting to a Local Explorer Node

When running the Explorer Node and Site on the same machine with a custom configuration, make sure the RPC URL and port in `site_config.toml` match the `rpc_listen` settings in `explorerd_config.toml` to establish connectivity with the node.

**Example Local Configuration Alignment**:

- **`site_config.toml`**:

  ```toml
  [localnet]
  explorer_rpc_url = "127.0.0.1"
  explorer_rpc_port = 14567
  ```

- **`explorerd_config.toml`**:

  ```toml
  [network_config."localnet"]
  rpc_listen = "tcp://127.0.0.1:14567"
  ```

### Running the Application

Launch the application using the Flask server:

```sh
FLASK_ENV=<environment> python -m flask run
```

Where `<environment>` can be:
- `localnet` - To run locally.
- `testnet` - For testing environment.
- `mainnet` - For mainnet environment.

### Logging

The Explorer Site provides logs that can be inspected to resolve issues and understand runtime behavior. The logging behavior adapts based on the environment setting. In `localnet`, logs are written to both console and files to assist with development and debugging, while `testnet` uses standard file handlers for log files only. For production use, `mainnet` employs rotating file handlers that maintain logs.

#### Log Locations

The log files are stored in environment-specific directories:

| Environment | Log Path |
|------------|----------|
| Localnet | `~/.local/share/darkfi/explorer_site/localnet` |
| Testnet | `~/.local/share/darkfi/explorer_site/testnet` |
| Mainnet | `~/.local/share/darkfi/explorer_site/mainnet` |

#### Log Output

The logging system maintains two log files in each environment directory:

| Log File | Purpose |
|----------|---------|
| `app.log` | Application logs and HTTP requests |
| `error.log` | Application errors |

#### Log Level Configuration

The logging level can be set using the `LOG_LEVEL` environment variable:
```sh
LOG_LEVEL=DEBUG FLASK_ENV=localnet python -m flask run
```