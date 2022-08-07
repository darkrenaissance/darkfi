# Error handling 

Before we continue, we need to quickly add some error handling to handle
the case where a user forgets to add the command-line flag.

```rust
{{#include ../../../../../example/dchat/src/dchat_error.rs:1:12}}
```

Finally we can read the flag from the command-line by adding the following lines to main():

```rust
{{#include ../../../../../example/dchat/src/main.rs:13:14}}
{{#include ../../../../../example/dchat/src/main.rs:17}}

{{#include ../../../../../example/dchat/src/main.rs:163:172}}
...
{{#include ../../../../../example/dchat/src/main.rs:197}}
```

