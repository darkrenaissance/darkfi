# Error handling 

Before we continue, we need to quickly add some error handling to handle
the case where a user forgets to add the command-line flag. We'll use a
Box<dyn error::Error> to implement that. Because we are now defining our own
Result type, we will need to remove `use darkfi::Result` from main.rs.

```
use std::{error, fmt};

pub type Error = Box<dyn error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct MissingSpecifier;

impl fmt::Display for MissingSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "missing node specifier. you must specify either a or b")
    }
}

impl error::Error for MissingSpecifier {}
```

Finally we can read the flag from the command-line by adding the following lines to main():

```
let settings: Result<Settings> = match std::env::args().nth(1) {
    Some(id) => match id.as_str() {
        "a" => alice(),
        "b" => bob(),
        _ => Err(MissingSpecifier.into()),
    },
    None => Err(MissingSpecifier.into()),
};
```

