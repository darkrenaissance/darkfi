# Darkmap

`Darkmap` aims to be a permissionless name system.
Anyone can have a number of pseudonyms, each pseudonym can own a number of
namespaces, and, later, transfer the ownerships if wished.

There is a strong gurantee of immutability, so values can be safely
cached locally even after transferring ownership.

The main application is to enable a private and secure software supply chain.

## Dpath

Syntax example: ns1:ns2.key

```
# mutable example
                   mutable
                      v
darkrenaissance:darkfi.master -> 1fb851750a6b8bfadfe60ca362cff0fc89a9b2ed (the HEAD changes frequently)
      ^           ^
   namespace    subnamespace


# immutable example

	  immutable immutable
               v      v
darkrenaissance:darkfi:v0_4_1 -> 0793fe32a3d7e9bedef9c3c0767647c74db215e9 (tagged commit should never change)
```

## Immutable ownership

```
darkrenaissance:darkfi:v0_4_1 -> 0793fe32a3d7e9bedef9c3c0767647c74db215e9 (tagged commit should never change)
                 ^
block_1:   namespace is owned by bob
					 
                                         
block_2:   namespace is owned by charlie, but because v0_4_1 is locked, charlie cannot change its value

In other words, immutability holds independent of namespace ownership
```

# Credit

Designed by someone else and with love.

