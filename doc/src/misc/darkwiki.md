# Darkwiki

Collaborative wiki using peer-to-peer network and raft consensus.

## Install

```shell
% git clone https://github.com/darkrenaissance/darkfi
% cd darkfi
% make BINS="darkwiki darkwikid"
% sudo make install BINS="darkwikid darkwiki"
```

## Usage

1 - Once Darkwiki get installed, darkwiki daemon must run in the background:

```shell
% darkwikid
```

2 - To update `synchronized directory` (default ~/darkwiki) and receive new documents from the network:

```shell
% darkwiki update
```

> **_NOTE:_**  The `synchronized directory` path can be changed from the config file in ~/.config/darkfi/darkwiki.toml

3 - After add/edit a document in ~/darkwiki, the changes will be published by running
  `update` command:

```shell
% darkwiki update
```

4 - For restore files having local changes to the original text: 

```shell
% darkwiki restore
```

5 - For both `restore` and `update` commands, the flag `--dry-run` can show the changes without applying/publishing the patches

```shell
% darkwiki update --dry-run
```

6 - Both `restore` and `update` commands are accepting passing the files names instead of updating/restoring all the documents in ~/darkwiki

```shell
% darkwiki update file1.md file2.md 
```



