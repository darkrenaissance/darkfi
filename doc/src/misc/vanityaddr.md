vanityaddr
==========

A tool for Vanity address generation for DarkFi keypairs, contract IDs,
and token IDs. Given some prefix, the tool will bruteforce secret keys
to find one which, when derived, starts with a given prefix.

## Usage

```
vanityaddr 0.4.1
Vanity address generation tool for DarkFi keypairs, contract IDs, and token IDs

Usage: vanityaddr [OPTIONS] <PREFIX> <PREFIX> ...

Arguments:
  <PREFIX>    Prefixes to search

Options:
  -c    Make the search case-sensitive
  -t    Number of threads to use (defaults to number of available CPUs)
  -A    Search for an address
  -C    Search for a Contract ID
  -T    Search for a Token ID
```

We can use the tool in our command line:

```
$ vanityaddr -A drk | jq
[1.214124215s] 53370 attempts
```

And the program will start crunching numbers. After a period of time,
we will get JSON output containing an address, secret key, and the
number of attempts it took to find the secret key.

```
{
  "address": "DRKN9N83iNs34YHu1RuW5nELvBSrV34JSztE64FR8DpX",
  "attempts": 30999,
  "secret": "9477oqchtHFMbCswnWqXptXGw9Ax1ynJN7SSLf346w6d"
}
```
