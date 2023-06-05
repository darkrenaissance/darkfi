# Installation

For now, you'd need to install maturin manually to run this tool.

```
# New vene and maturin
python3 -m venv ~/.venv-zkrunner
source ~/.venv-zkrunner/bin/activate
pip install maturin

# Install shared module onto Python
cd $DARKFI/src/sdk-py
maturin develop


# You can run zkrunner.py now!
```