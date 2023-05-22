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
instant_1:   owned by bob
instant 2:   owned by charlie now, but charlie cannot change what v0_4_1 points to

==============
implementation
==============

  say alice owns the top level darkrenaissance namespace and bob owns darkfi
  the resolution of darkrenaissance:darkfi:v0_4_1 will be:

            v
  1. darkrenaissance:darkfi:v0_4_1
     look up owner of darkrenaissance 
     get(h(1933, "darkrenaissance")) -> alice_account

		        v
  2. darkrenaissance:darkfi:v0_4_1
     get(h(alice_account, "darkfi")) -> bob_account

		               v 
  2. darkrenaissance:darkfi:v0_4_1
     get(h(bob_account, "v0_4_1"))   -> git commit hash 

===
log
===

 [DEBUG] (2) runtime::vm_runtime: Contract log: [SET] slot has no value                                                                                 
 [DEBUG] (2) runtime::vm_runtime: Contract log: [SET] slot  = 0x346c56bce1db9d6f08d045802add2d0e6c6406b1ea393fc5ea21c2dc417d0f32
 [DEBUG] (2) runtime::vm_runtime: Contract log: [SET] lock  = 0x0000000000000000000000000000000000000000000000000000000000000001
 [DEBUG] (2) runtime::vm_runtime: Contract log: [SET] value = 0x0000000000000000000000000000000000000000000000000000000000000004                        
 [DEBUG] (2) runtime::vm_runtime: Contract log: [SET] State update set!
 [DEBUG] (2) runtime::vm_runtime: Gas used: 1003160/200000000                                                                                           


======
credit
======

designed by someone else and with love

