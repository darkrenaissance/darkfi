=======
darkmap
=======

darkmap aims to be a permissionless name system.
anyone can have a number of pseudonyms, each pseudonym can own
a number of namespaces and, later, transfer the ownerships if wished.

there is a strong gurantee of immutability, so values can be safely
cached locally even after transferring ownership.

the main application is to enable a private and secure
software supply chain.

here is a dpath traversing some namespaces:

                   mutable
                      v
darkrenaissance:darkfi.master -> 1fb851750a6b8bfadfe60ca362cff0fc89a9b2ed (the HEAD changes frequently)
      ^           ^
   namespace    subnamespace


	  immutable immutable
               v      v
darkrenaissance:darkfi:v0_4_1 -> 0793fe32a3d7e9bedef9c3c0767647c74db215e9 (tagged commit should never change)
                 ^
instant_1:   owned by alice
instant 2:   owned by bob now, but bob cannot change what v0_4_1 points to

======
credit
======

designed by someone else and with love

