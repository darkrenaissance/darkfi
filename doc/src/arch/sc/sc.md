# Smart Contracts on DarkFi

This section of the book documents smart contract development.

## Wishlist

* Explicit FunctionId
* Invoke function from another function
* Function params use an ABI which is introspectable
    * This could be done using a separate schema which is loaded.
      See [Solidity's ABI spec](https://docs.soliditylang.org/en/latest/abi-spec.html).
    * This could be used to build the param args like in python with `**kwargs`.
* Backtrace accessible by functions, so they can check the parent caller.

