searchState.loadedDescShard("darkfi_money_contract", 0, "Smart contract implementing money transfers, atomic swaps, …\nPrecalculated root hash for a tree containing only a …\nzkas token auth mint circuit namespace\nzkas burn circuit namespace\nzkas fee circuit namespace\nzkas mint circuit namespace\nzkas token mint circuit namespace\nFunctions available in the contract\nClient API for interaction with this smart contract This …\nInternal contract errors\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCall parameters definitions\n<code>MoneyNote</code> holds the inner attributes of a <code>Coin</code>.\n<code>OwnCoin</code> is a representation of <code>Coin</code> with its respective …\n<code>Money::AuthTokenFreezeV1</code> API\n<code>Money::AuthTokenMintV1</code> API\nThe coin hash\nBlinding factor for the coin\n<code>Money::FeeV1</code> API\nReturns the argument unchanged.\nReturns the argument unchanged.\n<code>Money::GenesisMintV1</code> API\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCoin’s leaf position in the Merkle tree of coins\nAttached memo (arbitrary data)\nThe attached <code>MoneyNote</code>\nDerive the <code>Nullifier</code> for this <code>OwnCoin</code>\n<code>Money::PoWRewardV1</code> API\nCoin’s secret key\nSpend hook used for protocol-owned liquidity. Specifies …\n<code>Money::OtcSwapV1</code> API This API is crufty. Please rework it …\nBlinding factor for the token ID pedersen commitment\nToken ID of the coin\n<code>Money::TokenMintV1</code> API\n<code>Money::TransferV1</code> API\nUser data used by protocol when spend hook is enabled\nValue of the coin\nBlinding factor for the value pedersen commitment\nStruct holding necessary information to build a …\nProving key for the <code>AuthTokenMint_V1</code> zk circuit,\n<code>AuthTokenMint_V1</code> zkas circuit ZkBinary\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nMint authority keypair\nStruct holding necessary information to build a …\nProving key for the <code>AuthTokenMint_V1</code> zk circuit,\n<code>AuthTokenMint_V1</code> zkas circuit ZkBinary\nCoin attributes\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nMint authority keypair\nToken attributes\nFixed gas used by the fee call. This is the minimum gas …\nPrivate values related to the Fee call\nRevealed public inputs of the <code>Fee_V1</code> ZK proof\nSimultaneously blinds the coin and ensures uniqueness\nThe <code>OwnCoin</code> containing necessary metadata to create an …\nCreate the <code>Fee_V1</code> ZK proof given parameters\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nEncrypted user data for input coin\nThe value blind created for the input\nInput’s value commitment\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nMerkle path in the Money Merkle tree for <code>coin</code>\nMerkle root for input coin\nDecrypted note associated with the output\nInput’s Nullifier\nOutput coin commitment\nThe value blind created for the output\nOutput value commitment\nThe ZK proof created in this builder\nPublic key used to sign transaction\nThe ephemeral secret key created for tx signining\nTransform the struct into a <code>Vec&lt;pallas::Base&gt;</code> ready for …\nToken commitment\nThe blinding factor for user_data\nStruct holding necessary information to build a …\nAmount of tokens we want to mint\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nProving key for the <code>Mint_V1</code> zk circuit\n<code>Mint_V1</code> zkas circuit ZkBinary\nOptional recipient’s public key, in case we want to mint …\nCaller’s public key, corresponding to the one used in …\nOptional contract spend hook to use in the output\nOptional user data to use in the output\nStruct holding necessary information to build a …\nRewarded block height\nThis function should only be used for testing, as PoW …\nRewarded block transactions paid fees\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nProving key for the <code>Mint_V1</code> zk circuit\n<code>Mint_V1</code> zkas circuit ZkBinary\nOptional recipient’s public key, in case we want to mint …\nCaller’s public key, corresponding to the one used in …\nOptional contract spend hook to use in the output\nOptional user data to use in the output\nStruct holding necessary information to build a …\nProving key for the <code>Burn_V1</code> zk circuit\n<code>Burn_V1</code> zkas circuit ZkBinary\nThe coin to be used as the input to the swap\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nProving key for the <code>Mint_V1</code> zk circuit\n<code>Mint_V1</code> zkas circuit ZkBinary\nParty’s public key for receiving the output\nSpend hook for the party’s output\nThe blinds to be used for token ID pedersen commitments\nThe token ID of the party’s output to receive\nThe token ID of the party’s input to swap (send)\nMerkle tree of coins used to create inclusion proofs\nUser data blind for the party’s input\nUser data for the party’s output\nThe blinds to be used for value pedersen commitments <code>[0]</code> …\nThe value of the party’s output to receive\nThe value of the party’s input to swap (send)\nStruct holding necessary information to build a …\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nProving key for the <code>TokenMint_V1</code> zk circuit,\n<code>TokenMint_V1</code> zkas circuit ZkBinary\nStruct holding necessary information to build a …\nSimultaneously blinds the coin and ensures uniqueness\nProving key for the <code>Burn_V1</code> zk circuit\n<code>Burn_V1</code> zkas circuit ZkBinary\nClear inputs\nThe <code>OwnCoin</code> containing necessary metadata to create an …\nThe value blinds created for the inputs\nAnonymous inputs\nMake a simple anonymous transfer call.\nMerkle path in the Money Merkle tree for <code>coin</code>\nProving key for the <code>Mint_V1</code> zk circuit\n<code>Mint_V1</code> zkas circuit ZkBinary\nDecrypted notes associated with each output\nThe value blinds created for the outputs\nAnonymous outputs\nThe ZK proofs created in this builder\nSelect coins from <code>coins</code> of at least <code>min_value</code> in total. …\nThe ephemeral secret keys created for signing\nStruct holding necessary information to build a …\nSimultaneously blinds the coin and ensures uniqueness\nProving key for the <code>Burn_V1</code> zk circuit\n<code>Burn_V1</code> zkas circuit ZkBinary\nClear inputs\nThe <code>OwnCoin</code> containing necessary metadata to create an …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nThe value blinds created for the inputs\nAnonymous inputs\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nMerkle path in the Money Merkle tree for <code>coin</code>\nProving key for the <code>Mint_V1</code> zk circuit\n<code>Mint_V1</code> zkas circuit ZkBinary\nDecrypted notes associated with each output\nThe value blinds created for the outputs\nAnonymous outputs\nThe ZK proofs created in this builder\nThe ephemeral secret keys created for signing\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nA contract call’s clear input\nA <code>Coin</code> represented in the Money state\nA contract call’s anonymous input\nParameters for <code>Money::AuthTokenFreeze</code>\nState update for <code>Money::AuthTokenFreeze</code>\nParameters for <code>Money::AuthTokenMint</code>\nState update for <code>Money::AuthTokenMint</code>\nParameters for <code>Money::Fee</code>\nState update for <code>Money::Fee</code>\nParameters for <code>Money::GenesisMint</code>\nState update for <code>Money::GenesisMint</code>\nParameters for <code>Money::PoWReward</code>\nState update for <code>Money::PoWReward</code>\nParameters for <code>Money::TokenMint</code>\nState update for <code>Money::TokenMint</code>\nParameters for <code>Money::Transfer</code> and <code>Money::OtcSwap</code>\nState update for <code>Money::Transfer</code> and <code>Money::OtcSwap</code>\nA contract call’s anonymous output\nSimultaneously blinds the coin and ensures uniqueness\nMinted coin\nMinted coin\nThe newly minted coin\nThe newly minted coin\nThe newly minted coin\nThe newly minted coin\nMinted coins\nHeight accumulated fee paid\nFee value blind\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCreate a <code>Coin</code> object from given bytes, erroring if the …\nBlock height the fee was verified against\nBlock height the call was verified against\nReference the raw inner base field element\nAnonymous input\nClear input\nClear input\nAnonymous inputs\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nRevealed Merkle root\nMint authority public key\nAEAD encrypted note\nNullifier definitions\nRevealed nullifier\nRevealed nullifier\nRevealed nullifiers\nAnonymous outputs\nAnonymous output\nAnonymous output\nAnonymous outputs\nPublic key for the signature\nPublic key for the signature\nConvert the <code>Coin</code> type into 32 raw bytes\nBlinding factor for <code>token_id</code>\nToken ID blind\nCommitment for the input’s token ID\nCommitment for the output’s token ID\nToken ID definitions and methods\nInput’s token ID\nEncrypted user data field. An encrypted commitment to …\nInput’s value (amount)\nBlinding factor for <code>value</code>\nPedersen commitment for the input’s value\nPedersen commitment for the output’s value\nThe <code>Nullifier</code> is represented as a base field element.\nReturns the argument unchanged.\nCreate a <code>Nullifier</code> object from given bytes\nReference the raw inner base field element\nCalls <code>U::from(self)</code>.\nConvert the <code>Nullifier</code> type into 32 raw bytes\nNative DARK token ID. It does not correspond to any real …\nDerivation prefix for <code>TokenId</code>\nTokenId represents an on-chain identifier for a certain …\nDerives a <code>TokenId</code> from a <code>SecretKey</code> (mint authority)\nDerives a <code>TokenId</code> from a <code>PublicKey</code>\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCreate a <code>TokenId</code> object from given bytes, erroring if the …\nGet the inner <code>pallas::Base</code> element.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nConvert the <code>TokenId</code> type into 32 raw bytes")