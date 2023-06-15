# darkfi-sdk-py

## Bootstrap

This python sdk requires a virtual environment, along with a build tool.
To create the environment, execute:
```
$ make bootstrap
```
You may find more information in [pyo3](https://pyo3.rs/v0.15.1/#using-rust-from-python)
setup guide.

## Development

### Build

After successfully bootstrapping the virtual environment,
you can build the sdk by simply executing:
```
$ make
```

After all development is finished, you need to remove
the virtual envirnment folder, as it breaks rest make
operations in the repo, so just execute:
```
$ make clean
```

### Usage example

```
$ python3
from darkfi_sdk_py.base import Base
a = Base.from_u64(42)
b = Base.from_u64(69)
a + b == Base.from_u64(111)
```

### Randomness

Note that the `random` methods take randomness 
from the OS on the Rust side.
