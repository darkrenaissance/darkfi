BIN := venv/bin/activate
PYTHON := /opt/tools/python/bin/python3.12

all: $(BIN)

# Create the virtual environment and install dependencies
$(BIN): requirements.txt
	@echo "Installing dependencies..."
	@if [ ! -d venv ]; then \
		$(PYTHON) -m venv venv; \
	fi; \
	. venv/bin/activate && pip install -r requirements.txt

# Start the Flask server for the specified network (localnet, testnet, mainnet)
start-%: all
	@if [ "$*" != "localnet" ] && [ "$*" != "testnet" ] && [ "$*" != "mainnet" ]; then \
		echo "Error: Unsupported environment '$*'. Supported values are 'localnet', 'testnet', and 'mainnet'."; \
		exit 1; \
	fi
	@if [ -f flask.pid ]; then \
		echo "Flask server is already running (PID=$$(cat flask.pid)). Stop it first before starting."; \
		exit 1; \
	fi
	@. venv/bin/activate && \
		FLASK_ENV=$* python -m flask run & PID=$$!; \
		echo $$PID > flask.pid; \
		echo "Started flask server on $* network (PID=$$PID)"

# Stop the Flask server if running
stop-server:
	@if [ -f flask.pid ]; then \
		PID=$$(cat flask.pid); \
		kill $$PID; \
		rm -f flask.pid; \
		echo "Stopped Flask server"; \
	else \
		echo "Flask server is not running, nothing to stop."; \
	fi

# Install dependencies (alias for 'all')
install: all

# Remove virtual environment
clean:
	@rm -rf venv
	@echo "Cleaned the virtual environment!"

# Declare PHONY targets
.PHONY: all start-% install clean stop-server