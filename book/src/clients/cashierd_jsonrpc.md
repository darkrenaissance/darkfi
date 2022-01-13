# cashierd JSON-RPC API

## Methods
* [`deposit`](#deposit)
* [`withdraw`](#withdraw)
* [`features`](#features)


### `deposit`

Executes a deposit request given `network` and `token_id`.
Returns the address where the deposit shall be transferred to.
<br><sup><a href="https://github.com/darkrenaissance/darkfi/blob/master/bin/cashierd/src/main.rs#L331">[src]</a></sup>

```json
--> {"method": "deposit", "params": [network, token, publickey]}
<-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
```
### `withdraw`

Executes a withdraw request given `network`, `token_id`, `publickey`
and `amount`. `publickey` is supposed to correspond to `network`.
Returns the transaction ID of the processed withdraw.
<br><sup><a href="https://github.com/darkrenaissance/darkfi/blob/master/bin/cashierd/src/main.rs#L462">[src]</a></sup>

```json
--> {"method": "withdraw", "params": [network, token, publickey, amount]}
<-- {"result": "txID"}
```
### `features`

Returns supported cashier features, like network, listening ports, etc.
<br><sup><a href="https://github.com/darkrenaissance/darkfi/blob/master/bin/cashierd/src/main.rs#L549">[src]</a></sup>

```json
--> {"method": "features", "params": []}
<-- {"result": {"network": ["btc", "sol"]}
```
