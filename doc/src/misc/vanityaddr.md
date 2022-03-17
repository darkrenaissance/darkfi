vanityaddr
==========

A tool for Vanity address generation for DarkFi keypairs. Given some
prefix, the tool will bruteforce secret keys to find one which, when
derived into an address, starts with a given prefix.

## Usage

```
vanityaddr 0.3.0
Vanity address generation tool for DarkFi keypairs.

USAGE:
    vanityaddr [OPTIONS] <PREFIX>

ARGS:
    <PREFIX>    Prefix to search (must start with 1)

OPTIONS:
    -c                  Should the search be case-sensitive
    -h, --help          Print help information
    -t <THREADS>        Number of threads to use (defaults to number of available CPUs)
    -V, --version       Print version information
```

We can use the tool in our command line:

```
% vanityaddr 1Foo
[00:00:05] 53370 attempts
```

And the program will start crunching numbers. After a period of time,
we will get JSON output containing an address, secret key, and the
number of attempts it took to find the secret key.

```
{"address":"1FoomByzBBQywKaeBB5XPkAm5eCboh8K4CBhBe9uKbJm3kEiCS","attempts":78418,"secret":"0x16545da4a401adcd035ef51c8040acf5f4f1c66c0dd290bb5ec9e95991ae3615"}
```
