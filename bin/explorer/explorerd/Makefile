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
PROJECT_ROOT=../../..

# Directory where node logs are stored
LOG_HOME := $(shell echo ~/.local/share/darkfi/logs)

SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../../src -type f -name '*.rs') \

BIN = $(shell grep '^name = ' Cargo.toml | sed 1q | cut -d' ' -f3 | tr -d '"')

all: $(BIN)

help:
	@echo "Explorerd Makefile Commands:"
	@echo ""
	@echo "Build targets:"
	@echo "  make               - Build the $(BIN) binary"
	@echo "  make clean         - Remove build artifacts"
	@echo "  make install       - Install $(BIN) to $(PREFIX)/bin"
	@echo "  make uninstall     - Remove $(BIN) from $(PREFIX)/bin"
	@echo ""
	@echo "Network management:"
	@echo "  make start-localnet - Start the explorer node environment on localnet"
	@echo "  make start-testnet  - Start the explorer node environment on testnet"
	@echo "  make start-mainnet  - Start the explorer node environment on mainnet"
	@echo "  make stop           - Stop all nodes running within the explorer node environment"
	@echo ""
	@echo "Utility targets:"
	@echo "  make bundle_contracts_src - Bundle contract sources and ZK proofs into a tar archives in native_contracts_src directory"
	@echo "  make await-startup-NETWORK - Wait until nodes are ready (used in scripting, replace NETWORK with localnet/testnet/mainnet)"
	@echo ""
	@echo "Log files are stored in: $(LOG_HOME)/{localnet|testnet|mainnet}/"

$(BIN): $(SRC) bundle_contracts_src
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package $@
	cp -f ../../../target/$(RUST_TARGET)/release/$@ $@
	cp -f ../../../target/$(RUST_TARGET)/release/$@ ../../../$@

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release --package $(BIN)
	rm -f $(BIN) ../../../$(BIN)
	rm -rf native_contracts_src

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(BIN)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)

# Bundle native contract sources and their ZK proofs
bundle_contracts_src:
	@mkdir -p $(CURDIR)/native_contracts_src
	@(cd ../../../src && \
		tar -cf $(CURDIR)/native_contracts_src/deployooor_contract_src.tar -C contract/deployooor/src --transform 's,^./,,' . && \
		tar -cf $(CURDIR)/native_contracts_src/dao_contract_src.tar -C contract/dao/src --transform 's,^./,,' . 2>/dev/null && \
		find contract/dao/proof -name '*.zk' -exec tar -rf $(CURDIR)/native_contracts_src/dao_contract_src.tar --transform 's,^.*proof/,proof/,' {} + && \
		tar -cf $(CURDIR)/native_contracts_src/money_contract_src.tar -C contract/money/src --transform 's,^./,,' . && \
		find contract/money/proof -name '*.zk' -exec tar -rf $(CURDIR)/native_contracts_src/money_contract_src.tar --transform 's,^.*proof/,proof/,' {} + \
	)

# Start explorer on darkfid localnet
start-localnet: check-minerd

# Start localnet/testnet/mainnet networks
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
		$(if $(filter localnet,$*),$(PROJECT_ROOT)/minerd -c $(PROJECT_ROOT)/bin/minerd/minerd_config.toml & echo $$! >> PIDs.txt; sleep 2;) \
		$(PROJECT_ROOT)/darkfid --log "$$LOG_DIR/darkfid.log" -c $(PROJECT_ROOT)/bin/darkfid/darkfid_config.toml --network $* & echo $$! >> PIDs.txt; sleep 2; \
		$(call wait_for_darkfid_startup, $$LOG_DIR) \
		./explorerd --log "$$LOG_DIR/explorerd.log" -c ./explorerd_config.toml --network $* & echo $$! >> PIDs.txt; \
		$(call wait_for_explorerd_startup, $$LOG_DIR) \
	'

# Check and build darkfid if it does not exist
check-darkfid:
	@if [ ! -f $(PROJECT_ROOT)/darkfid ]; then \
		echo "Building darkfid..."; \
		$(MAKE) -C $(PROJECT_ROOT) darkfid; \
	fi

# Check and build explorerd if it does not exist
check-explorerd:
	@if [ ! -f ./explorerd ]; then \
		echo "Building explorerd..."; \
		$(MAKE) -C . ; \
	fi

# Check and build minerd if it does not exist
check-minerd:
	@if [ ! -f $(PROJECT_ROOT)/minerd ]; then \
		echo "Building minerd..."; \
		$(MAKE) -C $(PROJECT_ROOT) minerd; \
	fi

# Stop the running network
# Usage: make stop [suppress_not_running=1]
stop:
	@if [ -f PIDs.txt ]; then \
		while read PID; do \
			kill $$PID 2>/dev/null || echo "Unable to kill PID $$PID"; \
		done < PIDs.txt; \
		rm -f PIDs.txt; \
		sleep 5; \
		echo "Stopped explorer node environment"; \
	else \
		if [ "$(suppress_not_running)" != "1" ]; then \
			echo "Explorer node environment not running, nothing to stop."; \
		fi; \
	fi

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

# Waits for network to start
await-startup-%:
	@$(call wait_for_darkfid_startup,$(LOG_HOME)/$*)
	@$(call wait_for_explorerd_startup,$(LOG_HOME)/$*)

.PHONY: help all clean install uninstall bundle_contracts_src check-minerd check-darkfid check-explorerd stop start-%
