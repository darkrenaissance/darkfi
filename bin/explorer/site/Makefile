BIN := venv/bin/activate
PYTHON := python

LOG_HOME := $(shell echo ~/.local/share/darkfi/explorer_site)

help:
	@echo "Explorer Site Makefile Commands:"
	@echo ""
	@echo "Installation and cleanup:"
	@echo "  make install       - Install Python dependencies in a virtual environment"
	@echo "  make clean         - Remove the virtual environment and installed dependencies"
	@echo ""
	@echo "Server management:"
	@echo "  make start-localnet - Start explorer site on localnet environment"
	@echo "  make start-testnet  - Start explorer site on testnet environment"
	@echo "  make start-mainnet  - Start explorer site on mainnet environment"
	@echo "  make stop           - Stop running explorer site"

install: $(BIN)

# Create the virtual environment and install dependencies
$(BIN): requirements.txt
	@echo "Installing dependencies..."
	@if [ ! -d venv ]; then \
		$(PYTHON) -m venv venv; \
	fi; \
	. venv/bin/activate && pip install -r requirements.txt

# Remove virtual environment
clean:
	@rm -rf venv
	@echo "Cleaned the virtual environment!"

# Start the Flask server for the specified network (localnet, testnet, mainnet)
start-%: install
	@if [ "$*" != "localnet" ] && [ "$*" != "testnet" ] && [ "$*" != "mainnet" ]; then \
		echo "Error: Unsupported environment '$*'. Supported values are 'localnet', 'testnet', and 'mainnet'."; \
		exit 1; \
	fi
	@if [ -f flask.pid ]; then \
		echo "Explorer site is already running (PID=$$(cat flask.pid)). Stop it first before starting."; \
		exit 1; \
	fi
	@. venv/bin/activate && \
		FLASK_ENV=$* python -m flask run & PID=$$!; \
		echo $$PID > flask.pid; \
		echo "Started explorer site on $* network (PID=$$PID)"

	@if [ "$*" = "testnet" ]; then \
		echo "See site logfile $(LOG_HOME)/testnet/app.log for server startup details"; \
	fi

	@if [ "$*" = "mainnet" ]; then \
		echo "See site logfile $(LOG_HOME)/mainnet/app.log for server startup details"; \
	fi

# Stop the explorer site if running
stop:
	@if [ -f flask.pid ]; then \
		PID=$$(cat flask.pid); \
		kill $$PID; \
		rm -f flask.pid; \
		echo "Stopped explorer site"; \
	else \
		if [ "$(suppress_not_running)" != "1" ]; then \
			echo "Explorer site is not running, nothing to stop."; \
		fi; \
	fi

# Declare PHONY targets
.PHONY: all start-% install clean stop-server