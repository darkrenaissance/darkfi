# JSONRPC API 
## Methods 
- [say_hello](jsonrpc.md#say_hello)
- [create_wallet](jsonrpc.md#create_wallet)
- [key_gen](jsonrpc.md#key_gen)
- [get_key](jsonrpc.md#get_key)
- [get_keys](jsonrpc.md#get_keys)
- [import_keypair](jsonrpc.md#import_keypair)
- [export_keypair](jsonrpc.md#export_keypair)
- [set_default_address](jsonrpc.md#set_default_address)
- [get_balances](jsonrpc.md#get_balances)
- [get_token_id](jsonrpc.md#get_token_id)
- [features](jsonrpc.md#features)
- [deposit](jsonrpc.md#deposit)
- [withdraw](jsonrpc.md#withdraw)
- [transfer](jsonrpc.md#transfer)
### say_hello: 
`params`: []

`result`: "helloworld"

### create_wallet: 
`params`: []

`result`: true

### key_gen: 
`params`: []

`result`: true

### get_key: 
`params`: []

`result`: "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"

### get_keys: 
`params`: []

`result`: "[vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC, ...]"

> `note`:  the first address in the returned vector is the default address

### import_keypair: 
`params`: [path]

`result`: true

### export_keypair: 
`params`: [path]

`result`: true

### set_default_address: 
`params`: [vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC]

`result`: true

### get_balances: 
`params`: []

`result`: "[{"btc":(value, network)}, ...]"

### get_token_id: 
`params`: [network, token]

`result`: "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"

### features: 
`params`: []

`result`: {"network":["btc", "sol"]}

### deposit: 
`params`: [network, token, publickey]

`result`: "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"

> `note`:  The publickey sent here is used so the cashier can know where to send
 tokens once the deposit is received.

### withdraw: 
`params`: [network, token, publickey, amount]

`result`: "txID"

> `note`:  The publickey sent here is the address where the caller wants to receive
 the tokens they plan to withdraw.
 On request, send request to cashier to get deposit address, and then transfer
 dark tokens to the cashier's wallet. Following that, the cashier should return
 a transaction ID of them sending the funds that are requested for withdrawal.

### transfer: 
`params`: [network, dToken, address, amount]

`result`: "txID"

