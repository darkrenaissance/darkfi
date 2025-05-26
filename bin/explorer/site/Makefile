BIN := venv/bin/activate
PYTHON := python

export LOG_HOME := $(shell echo ~/.local/share/darkfi/explorer_site)

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
	. venv/bin/activate && pip install -q -r requirements.txt

# Remove virtual environment
clean:
	@rm -rf venv
	@echo "Cleaned the virtual environment!"

# Start the server for the specified network (localnet, testnet, mainnet)
start-%: install
	@if [ "$*" != "localnet" ] && [ "$*" != "testnet" ] && [ "$*" != "mainnet" ]; then \
		echo "Error: Unsupported environment '$*'. Supported values are 'localnet', 'testnet', and 'mainnet'."; \
		exit 1; \
	fi
	@if [ -f flask.pid ] && [ "$*" = "localnet" ]; then \
		echo "Explorer site is already running (PID=$$(cat flask.pid)). Stop it first before starting."; \
		exit 1; \
	elif [ -f gunicorn.pid ] && { [ "$*" = "testnet" ] || [ "$*" = "mainnet" ]; }; then \
		echo "Explorer site is already running (PID=$$(cat gunicorn.pid)). Stop it first before starting."; \
		exit 1; \
	fi
	@. venv/bin/activate && if [ "$*" = "testnet" ] || [ "$*" = "mainnet" ]; then \
		FLASK_ENV=$* gunicorn --config gunicorn_config.py wsgi:app & PID=$$!; \
		echo $$PID > gunicorn.pid; \
		echo "Explorer site started on $* network (PID=$$PID)"; \
		if [ "$*" = "testnet" ]; then \
			echo "See site logfile $(LOG_HOME)/testnet/app.log for server startup details"; \
		fi; \
		if [ "$*" = "mainnet" ]; then \
			echo "See site logfile $(LOG_HOME)/mainnet/app.log for server startup details"; \
		fi; \
	else \
		FLASK_ENV=$* python -m flask run & PID=$$!; \
		echo $$PID > flask.pid; \
		echo "Started explorer site on $* network (PID=$$PID)"; \
	fi

# Stop the explorer sites that are running
stop:
	@if [ -f flask.pid ] || [ -f gunicorn.pid ]; then \
		[ -f flask.pid ] && kill $$(cat flask.pid) 2>/dev/null || true; \
		rm -f flask.pid; \
		[ -f gunicorn.pid ] && kill $$(cat gunicorn.pid) 2>/dev/null || true; \
		rm -f gunicorn.pid; \
		echo "Stopped running explorer sites"; \
	else \
		echo "An explorer site is not running, nothing to stop."; \
	fi

# Declare PHONY targets
.PHONY: all start-% install clean stop-server