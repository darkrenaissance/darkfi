# darkfid JSON-RPC API

## Methods
* [`say_hello`](#say_hello)
* [`create_wallet`](#create_wallet)
* [`key_gen`](#key_gen)
* [`get_key`](#get_key)
* [`get_keys`](#get_keys)
* [`import_keypair`](#import_keypair)
* [`export_keypair`](#export_keypair)
* [`set_default_address`](#set_default_address)
* [`get_balances`](#get_balances)
* [`get_token_id`](#get_token_id)
* [`features`](#features)
* [`deposit`](#deposit)
* [`withdraw`](#withdraw)
* [`transfer`](#transfer)


### `say_hello`

Returns a `helloworld` string.

```json
--> {"method": "say_hello", "params": []}
<-- {"result": "helloworld"}
```
### `create_wallet`

Attempts to initialize a wallet, and returns `true` upon success.

```json
--> {"method": "create_wallet", "params": []}
<-- {"result": true}
```
### `key_gen`

Attempts to generate a new keypair and returns `true` upon success.

```json
--> {"method": "key_gen", "params": []}
<-- {"result": true}
```
### `get_key`

Fetches the main keypair from the wallet and returns it
in an encoded format.

```json
--> {"method": "get_key", "params": []}
<-- {"result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"}
```
### `get_keys`

Fetches all keypairs from the wallet and returns a list of them
in an encoded format.
The first one in the list is the default selected keypair.

```json
--> {"method": "get_keys", "params": []}
<-- {"result": "[vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC,...]"}
```
### `import_keypair`

Imports a keypair into the wallet with a given path on the filesystem.
Returns `true` upon success.

```json
--> {"method": "import_keypair", "params": [path]}
<-- {"result": true}
```
### `export_keypair`

Exports the default selected keypair to a given path on the filesystem.
Returns `true` upon success.

```json
--> {"method": "export_keypair", "params": [path]}
<-- {"result": true}
```
### `set_default_address`

Sets the default wallet address to the given parameter.
Returns true upon success.

```json
--> {"method": "set_default_address", "params": [vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC]}
<-- {"result": true}
```
### `get_balances`

Fetches the known balances from the wallet.
Returns a map of balances, indexed by `network`, and token ID.

```json
--> {"method": "get_balances", "params": []}
<-- {"result": "[{"btc":(value,network)},...]"}
```
### `get_token_id`

Generates the internal token ID for a given `network` and token ticker or address.
Returns the internal representation of the token ID.

```json
--> {"method": "get_token_id", "params": [network,token]}
<-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
```
### `features`

Asks the configured cashier for their supported features.
Returns a map of features received from the requested cashier.

```json
--> {"method": "features", "params": []}
<-- {"result": {"network":["btc","sol"]}}
```
### `deposit`

Initializes a DarkFi deposit request for a given `network`, `token`,
and `publickey`.
The public key send here is used so the cashier can know where to send
the newly minted tokens once the deposit is received.
Returns an address to which the caller is supposed to deposit funds.

```json
--> {"method": "deposit", "params": [network,token,publickey]}
<-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
```
### `withdraw`

Initializes a withdraw request for a given `network`, `token`, `publickey`,
and `amount`.
The publickey send here is the address where the caller wants to receive
the tokens they plan to withdraw.
On request, sends a request to a cashier to get a deposit address, and
then transfers wrapped DarkFitokens to the cashier's wallet. Following that,
the cashier should return a transaction ID of them sending the funds that
are requested for withdrawal.

```json
--> {"method": "withdraw", "params": [network,token,publickey,amount]}
<-- {"result": "txID"}
```
### `transfer`

Transfer a given wrapped DarkFi token amount to the given address.
Returns the transaction ID of the transfer.

```json
--> {"method": "transfer", "params": [network,dToken,address,amount]}
<-- {"result": "txID"}
```
