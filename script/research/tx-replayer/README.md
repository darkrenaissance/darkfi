# Tx-Replayer

A lightweight transaction replay tool for debugging and analyzing
transactions, as well as measuring resource usage during transaction
verification with profiler tools such as
[heaptrack](https://github.com/KDE/heaptrack) and
[samply](https://github.com/mstange/samply).

**Disclaimer:** Use this tool only on a copy of your database.
Running it on a live database may cause data loss or corruption.

## Usage
Build
```
% make
```
To replay the whole transaction verification step.
```
% ./tx-replayer --database-path [DATABASE_PATH] --tx-hash [TX_HASH]
```
To replay only the Zk proof verification part.
```
% ./tx-replayer --zkp --database-path [DATABASE_PATH] --tx-hash [TX_HASH]
```
To replay only the wasm Runtime verification part.
```
% ./tx-replayer --wasm --database-path [DATABASE_PATH] --tx-hash [TX_HASH]
```
To replay only the signature verification part.
```
% ./tx-replayer --sig --database-path [DATABASE_PATH] --tx-hash [TX_HASH]
```
You can run `samply` to see the CPU usage of the transaction verification.
```
% samply record ./tx-replayer --database-path [DATABASE_PATH] --tx-hash [TX_HASH]
```
