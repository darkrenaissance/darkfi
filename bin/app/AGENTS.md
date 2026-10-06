* Run `make fmt` to format code.
* Run `make compile-dev` to test desktop target.
* Run `make compile-apk` to test mobile target.
* Never use `crate::` inline in code. Always do an appropriate import.
  Imports must always be clustered together so no duplicate `use crate::` lines.
* `use parking_lot::Mutex as SyncMutex;` is the canonical sync mutex.
  `use async_lock::Mutex as AsyncMutex;` is the canonical async mutex.
* Internal logic errors should never fail gracefully. Always use `.unwrap()` to
  enforce explicit failure. Error handling is for the topmost layers coming from
  the edge of the app such as user input.

