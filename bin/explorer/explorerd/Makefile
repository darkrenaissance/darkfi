.POSIX:

# Suppress all directory-related messages for cleaner output
MAKEFLAGS += --no-print-directory

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# Root directory of the project
PROJECT_ROOT = ../../..

# Directory where logs are stored
LOG_HOME := $(shell echo ~/.local/share/darkfi/logs)

# This is the main binary built by the Makefile.
# Its corresponding configuration file is used to start the node.
EXPLORERD_BIN    := $(shell grep '^name = ' Cargo.toml | sed 1q | cut -d' ' -f3 | tr -d '"')
EXPLORERD_CONFIG := $(EXPLORERD_BIN)_config.toml

# If these binaries are missing when launching the explorer node environment,
# the Makefile (via a sub-make call) will build them if needed. Their configurations
# are used to start the nodes.
MINERD_BIN       := $(PROJECT_ROOT)/minerd
MINERD_CONFIG    := $(PROJECT_ROOT)/bin/minerd/minerd_config.toml
DARKFID_BIN      := $(PROJECT_ROOT)/darkfid
DARKFID_CONFIG   := $(PROJECT_ROOT)/bin/darkfid/darkfid_config.toml

# Source to build
SRC = \
	Cargo.toml \
	$(PROJECT_ROOT)/Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find $(PROJECT_ROOT)/src -type f -name '*.rs') \

all: $(EXPLORERD_BIN)

help:
	@echo "Explorerd Makefile Commands:"
	@echo ""
	@echo "Build targets:"
	@echo "  make               - Build the $(EXPLORERD_BIN) binary"
	@echo "  make clean         - Remove build artifacts"
	@echo "  make install       - Install $(EXPLORERD_BIN) to $(PREFIX)/bin"
	@echo "  make uninstall     - Remove $(EXPLORERD_BIN) from $(PREFIX)/bin"
	@echo ""
	@echo "Network management:"
	@echo "  make start-localnet - Start the explorer node environment on localnet"
	@echo "  make start-testnet  - Start the explorer node environment on testnet"
	@echo "  make start-mainnet  - Start the explorer node environment on mainnet"
	@echo "  make stop           - Stop all nodes running within the explorer node environment"
	@echo ""
	@echo "Utility targets:"
	@echo "  make bundle_contracts_src - Bundle contract sources and ZK proofs into tar archives in native_contracts_src directory"
	@echo "  make await-startup-{network} - Wait until nodes are ready (used in scripting, replace {network} with localnet/testnet/mainnet)"
	@echo ""
	@echo "Log files are stored in: $(LOG_HOME)/{localnet|testnet|mainnet}/"

$(EXPLORERD_BIN): $(SRC) bundle_contracts_src
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package $@
	cp -f $(PROJECT_ROOT)/target/$(RUST_TARGET)/release/$@ $@
	cp -f $(PROJECT_ROOT)/target/$(RUST_TARGET)/release/$@ $(PROJECT_ROOT)/$@

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release --package $(EXPLORERD_BIN)
	rm -f $(EXPLORERD_BIN) $(PROJECT_ROOT)/$(EXPLORERD_BIN)
	rm -rf native_contracts_src

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(EXPLORERD_BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(EXPLORERD_BIN)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(EXPLORERD_BIN)

# Bundles contract sources and ZK proofs into tar archives, supporting both GNU and BSD tar
bundle_contracts_src:
	@TMP_DIR="native_contracts_src/tmp"; \
	CONTRACT_SRC_DIR="$(PROJECT_ROOT)/src/contract"; \
	CONTRACTS="deployooor money dao"; \
	set -e; \
	# Create a temporary directory and clean up on exit \
	mkdir -p "$$TMP_DIR"; \
	trap "rm -rf $$TMP_DIR" 0 1 2 15; \
	# Bundle each native contract \
	for contract in $$CONTRACTS; do \
		PROOF_DIR="$$TMP_DIR/$$contract/proof"; \
		mkdir -p "$$TMP_DIR/$$contract"; \
		cp -R "$$CONTRACT_SRC_DIR/$$contract/src/"* "$$TMP_DIR/$$contract"; \
		# Include zk proofs if they exist \
		if [ -d "$$CONTRACT_SRC_DIR/$$contract/proof" ]; then \
			mkdir -p "$$PROOF_DIR"; \
			find "$$CONTRACT_SRC_DIR/$$contract/proof" -type f ! -name '*.bin' -exec cp {} "$$PROOF_DIR/" \; ; \
			# Move witness files to their own directory if present  \
			if find "$$PROOF_DIR" -name '*.json' | grep -q .; then \
				mkdir -p "$$PROOF_DIR/witness"; \
				mv "$$PROOF_DIR"/*.json "$$PROOF_DIR/witness"; \
			fi; \
		fi; \
		(cd "$$TMP_DIR/$$contract" && tar --format=pax -cf ../../$${contract}_contract_src.tar *); \
	done;

# Start explorer on localnet (requires minerd)
start-localnet: check-minerd

# Starts an explorer node environment for `localnet`, `testnet`, or `mainnet` networks.
# It validates the input network, stops any currently running nodes, initializes the log directory,
# optionally starts `minerd` for `localnet`, and starts the remaining nodes while waiting for them
# to properly initialize.
start-%: check-darkfid check-explorerd
	@if [ "$*" != "localnet" ] && [ "$*" != "testnet" ] && [ "$*" != "mainnet" ]; then \
		echo "Error: Unsupported network '$*'. Use 'localnet', 'testnet', or 'mainnet'."; \
		exit 1; \
	fi
	@$(MAKE) stop suppress_not_running=1
	@echo "Starting explorer node environment $*..."
	@sh -c ' \
		LOG_DIR=$(LOG_HOME)/$*; \
		mkdir -p "$$LOG_DIR"; \
		$(if $(filter localnet,$*), $(MINERD_BIN) -c $(MINERD_CONFIG) & echo $$! >> PIDs.txt; sleep 2;) \
		$(DARKFID_BIN) --log "$$LOG_DIR/darkfid.log" -c $(DARKFID_CONFIG) --network $* & echo $$! >> PIDs.txt; sleep 2; \
		$(call wait_for_darkfid_startup, $$LOG_DIR) \
		./$(EXPLORERD_BIN) --log "$$LOG_DIR/explorerd.log" -c $(EXPLORERD_CONFIG) --network $* & echo $$! >> PIDs.txt; \
		$(call wait_for_explorerd_startup, $$LOG_DIR) \
	'

# Starts an explorer node in no-sync mode for `localnet`, `testnet`, or `mainnet` networks.
# In no-sync mode, no connections are made with the Darkfi blockchain network and the explorer
# relies solely on a local database without attempting synchronization. As with the `start-%`
# target, inputs are verified, any running nodes are stopped, the log directory is initialized,
# and the node is started.
start-no-sync-%: check-contracts check-explorerd
	@if [ "$*" != "localnet" ] && [ "$*" != "testnet" ] && [ "$*" != "mainnet" ]; then \
		echo "Error: Unsupported network '$*'. Use 'localnet', 'testnet', or 'mainnet'."; \
		exit 1; \
	fi
	@$(MAKE) stop suppress_not_running=1
	@echo "Starting explorer node environment (no sync) $*..."
	@sh -c ' \
		LOG_DIR=$(LOG_HOME)/$*; \
		mkdir -p "$$LOG_DIR"; \
		./$(EXPLORERD_BIN) --log "$$LOG_DIR/explorerd.log" -c $(EXPLORERD_CONFIG) --network $* --no-sync & echo $$! >> PIDs.txt; \
		$(call wait_for_explorerd_startup, $$LOG_DIR) \
	'

# Check and build darkfid if it does not exist
check-darkfid:
	@if [ ! -f "$(DARKFID_BIN)" ]; then \
		echo "Building darkfid..."; \
		$(MAKE) -C "$(PROJECT_ROOT)" darkfid; \
	fi

# Check and build explorerd if it does not exist
check-explorerd:
	@if [ ! -f "$(EXPLORERD_BIN)" ]; then \
		echo "Building explorerd..."; \
		$(MAKE) -C .; \
	fi

# Check and build minerd if it does not exist
check-minerd:
	@if [ ! -f "$(MINERD_BIN)" ]; then \
		echo "Building minerd..."; \
		$(MAKE) -C "$(PROJECT_ROOT)" minerd; \
	fi

# Check and build contracts if they do not exist
check-contracts:
	@if [ ! -f "$(PROJECT_ROOT)/src/contract/money/darkfi_money_contract.wasm" ] \
	  || [ ! -f "$(PROJECT_ROOT)/contract/dao/darkfi_dao_contract.wasm" ] \
	  || [ ! -f "$(PROJECT_ROOT)/contract/deployooor/darkfi_deployooor_contract.wasm" ]; then \
		echo "Building contracts..."; \
		$(MAKE) -C "$(PROJECT_ROOT)" contracts; \
	fi

# Stop the running network
# Usage: make stop [suppress_not_running=1]
stop:
	@if [ -f PIDs.txt ]; then \
		while read PID; do \
			if ps -p $$PID > /dev/null 2>&1; then \
				kill -15 $$PID 2>/dev/null; \
				sleep 5; \
				ps -p $$PID > /dev/null 2>&1 && kill -9 $$PID 2>/dev/null; \
			fi; \
		done < PIDs.txt; \
		rm -f PIDs.txt; \
		echo "Stopped explorer node environment"; \
	else \
		if [ "$(suppress_not_running)" != "1" ]; then \
			echo "Explorer node environment not running, nothing to stop."; \
		fi; \
	fi

# Wait for the network to start
await-startup-%:
	@$(call wait_for_darkfid_startup,$(LOG_HOME)/$*)
	@$(call wait_for_explorerd_startup,$(LOG_HOME)/$*)

# Wait for the explorer node environment to start in no-sync mode (skip waiting for darkfid startup)
await-startup-no-sync-%:
	@$(call wait_for_explorerd_startup,$(LOG_HOME)/$*)

# Waits for Darkfid to start
define wait_for_darkfid_startup
  log_dir=$(strip $(1)); \
  while ! grep -q "Darkfid P2P handler started successfully!" "$$log_dir/darkfid.log" 2>/dev/null; do \
    sleep 1; \
  done;
endef

# Waits for Explorerd to start
define wait_for_explorerd_startup
  log_dir=$(strip $(1)); \
  while ! grep -q "Started DarkFi Explorer Node" "$$log_dir/explorerd.log" 2>/dev/null; do \
    sleep 1; \
  done;
endef

.PHONY: help all clean install uninstall bundle_contracts_src check-minerd check-darkfid check-explorerd stop start-%