# Getting started

We'll create a new cargo directory and add DarkFi to our `Cargo.toml`,
like so:

```
{{#include ../../../../../example/dchat/dchatd/Cargo.toml:darkfi}}
```

Be sure to replace the path to DarkFi with the correct path for your
setup.

Once that's done we can access DarkFi's net methods inside of
dchat. We'll need a few more external libraries too, so add these
dependencies:

```
{{#include ../../../../../example/dchat/dchatd/Cargo.toml:dependencies}}
```


