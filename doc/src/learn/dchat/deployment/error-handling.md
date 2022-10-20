# Error handling 

Before we continue, we need to quickly add some error handling to handle
the case where a user forgets to add the command-line flag.

```rust
{{#include ../../../../../example/dchat/src/dchat_error.rs:error}}
```

We can then read the flag from the command-line by adding the following
lines to `main()`:

```rust
use crate::dchat_error::ErrorMissingSpecifier;
use darkfi::net::Settings;

{{#include ../../../../../example/dchat/src/main.rs:error}}

async fn main() -> Result<()> {
    // ...
    let settings: Result<Settings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(ErrorMissingSpecifier.into()),
        },
        None => Err(ErrorMissingSpecifier.into()),
    };
    // ...
}
```

