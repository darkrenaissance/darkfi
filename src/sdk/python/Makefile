.POSIX:

# Pip install command
PIP = pip install

# Maturin binary
MATURIN = maturin

# Python dependencies
DEPS = $(MATURIN) patchelf

# WARNING:
# After all development here
# we need to remove the venv folder,
# as it breaks rest make operations
# in the repo, so just run `make clean`.
VENV = venv

# Venv setup
VENV_SETUP = python3 -m $(VENV) $(VENV)

all:
	$(MATURIN) develop --release

bootstrap: clean
	$(VENV_SETUP)	
	. $(VENV)/bin/activate
	$(PIP) $(DEPS)

clean:
	rm -rf $(VENV)

.PHONY: all bootstrap clean
