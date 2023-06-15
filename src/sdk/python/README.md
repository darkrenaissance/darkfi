# darkfi-sdk-py

Python bindings for some parts of the `darkfi-sdk` and the `zkvm`.

## Build and install

1. Install `maturin` via your package manager or from whatever source.
2. Run `make` to build the wheel
3. (Optional) Run pip install --user <path_to_wheel>

## Development

For a development version you can use a venv:

```
$ python3 -m venv venv
$ source venv/bin/activate
(venv) $ maturin develop --release
```

## Usage

```
$ python3
>>> import darkfi_sdk_py
>>> darkfi_sdk_py.__dir__()
>>>
>>> from darkfi_sdk_py.base import Base
>>> a = Base.from_u64(42)
>>> b = Base.from_u64(69)
>>> a + b == Base.from_u64(111)
```

### Randomness

Note that the `random` methods take randomness 
from the OS on the Rust side.
