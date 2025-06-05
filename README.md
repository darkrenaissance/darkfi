# Running a DarkFi Testnet Node: A Step-by-Step Guide 🚀

On June 5, 2025, after two days of debugging, I successfully set up and synced a DarkFi testnet node with my wallet! 🎉 This guide walks you through the process of running your own DarkFi testnet node, inspired by my journey. Follow [DarkFi on X](https://x.com/DarkFiProject) for updates, and check out their [official documentation](https://github.com/darkrenaissance/darkfi?tab=readme-ov-file) for more details. You can also see my original thread on X: [Provic 🌿 (@provic44)](https://x.com/provic44/status/1930605505635844360?t=3Ux-hvJeSimBA7f9XZdMHg&s=19).

DarkFi is a Layer 1 blockchain focused on anonymity, using zero-knowledge cryptography to enable uncensorable applications. Running a testnet node is a great way to explore decentralized tech and contribute to the #Web3 ecosystem! 🌐

## Prerequisites

Before starting, ensure you have:
- A Debian/Ubuntu-based system (other distros may require adjustments).
- Root or sudo access for installing dependencies.
- A stable internet connection for blockchain syncing.
- At least 2 GB of RAM and 10 GB of free disk space (recommended).

## Setup Steps

### 1. Install Dependencies
Update your system and install essential tools:
```bash
sudo apt update && sudo apt install -y build-essential libssl-dev pkg-config git cmake
```

### Step 2: Cloning the DarkFi Repository 📦

Clone the DarkFi repository and navigate into the directory:
```bash
git clone https://github.com/darkrenaissance/darkfi
cd darkfi
```


### Step 3: Install Rust and Toolchain

🛠️ Step 3 of my DarkFi testnet node setup: Installing Rust! 🦀 Ran `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` and sourced the environment. Then added WebAssembly with `rustup target add wasm32-unknown-unknown` and updated Rust.

Installing Rust for DarkFi 🦀  
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup target add wasm32-unknown-unknown
rustup update
```
### Step 4: Installing Additional Dependencies (Debian/Ubuntu) 📦

Install required packages:
```bash
sudo apt update
sudo apt install -y git cmake make gcc g++ pkg-config libasound2-dev libclang-dev libfontconfig1-dev liblzma-dev libssl-dev libsqlcipher-dev libsqlite3-dev wabt
```
### Step 5: Building the DarkFi Project 🛠️

Compile the project:
```bash
make
```
### Step 6: Setting Up DarkFi Configuration ⚙️

Create a config directory and edit the file:
```bash
mkdir -p ~/.config/darkfi
nano ~/.config/darkfi/drk_config.toml
```
Add:
```bash
[network_config."testnet"]
endpoint = "tcp://127.0.0.1:8240"
wallet_path = "/root/.local/share/drk/wallet"
wallet_pass = "My_password"
```
Save and exit (Ctrl+O, Enter, Ctrl+X in nano).


### Step 7: Running the DarkFi Node 🌐

Start the DarkFi daemon:
```bash
./darkfid
```
