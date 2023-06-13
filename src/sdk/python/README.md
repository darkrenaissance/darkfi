# Installation

Follow virtualenv and pyo3 setup guide: https://pyo3.rs/v0.15.1/#using-rust-from-python

## tldr

```
$ python3 -m venv venv
$ source venv/bin/activate
$ maturin develop --release
```

```
$ python3
from darkfi_sdk_py.base import Base
a = Base.from_u64(42)
b = Base.from_u64(69)
a + b == Base.from_u64(111)
```
