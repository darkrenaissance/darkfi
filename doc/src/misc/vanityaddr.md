vanityaddr
==========

A tool for Vanity address generation for DarkFi keypairs and token IDs.
Given some prefix, the tool will bruteforce secret keys to find one
which, when derived, starts with a given prefix.

## Usage

```
vanityaddr 0.4.1
Vanity address generation tool for DarkFi keypairs and token IDs

Usage: vanityaddr [OPTIONS] [PREFIX]...

Arguments:
  [PREFIX]...  Prefixes to search

Options:
  -c                 Should the search be case-sensitive
      --address      Search for an Address
      --token-id     Search for a Token ID
      --contract-id  Search for a Contract ID
  -t <THREADS>       Number of threads to use (defaults to number of available CPUs)
  -h, --help         Print help
  -V, --version      Print version
```

We can use the tool in our command line:

```
% vanityaddr drk
[00:00:05] 53370 attempts
```

And the program will start crunching numbers. After a period of time,
we will get JSON output containing an address, secret key, and the
number of attempts it took to find the secret key.

```
{"address":"DrkZcAiZPQoQUrdii9CUCQC2SNcUrSYEYW4wTj6Nhtp1","attempts":78418,"secret":"BL9zmxqFhCHHU42CPY1G4hj1ahUYh61F54rPBBwLVLVv"}
```
