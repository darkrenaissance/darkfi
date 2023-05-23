# Darkmap

`Darkmap` aims to be a permissionless name system.

Anyone can have a number of pseudonyms, each pseudonym can own a number of
namespaces.

There is a strong gurantee of immutability, so values can be safely
cached locally (even the secret is leaked).

The main application is to enable a private and secure software supply chain.

## Dpath

Syntax example: ns1:ns2.key

```
# mutable example

we want master to change, so we make it mutable

                   mutable
                      v
darkrenaissance:darkfi.master -> 1fb851750a6b8bfadfe60ca362cff0fc89a9b2ed 
      ^           ^
   namespace    subnamespace


# immutable example

once we cut the release tag, we don't want the path to change, so we make it immutable

	  immutable immutable
               v      v
darkrenaissance:darkfi:v0_4_1 -> 0793fe32a3d7e9bedef9c3c0767647c74db215e9 (tagged commit should never change)
```

```
darkrenaissance:darkfi:v0_4_1 -> 0793fe32a3d7e9bedef9c3c0767647c74db215e9 (tagged commit should never change)
                 ^
       namespace is owned by bob 
       suppose bob's secret is leaked
       because v0_4_1 is permanently locked in the darkfi namespace,
       adversary cannot change the path's value

```

# Credit

Designed by someone else and with love.

