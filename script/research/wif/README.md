# Wallet import format

base58 encoding of extended address, and optional checksum.

# Checksum

Last 4 bytes of double sha256 encoding of extended address

# Extended address

Extended address consist of prefix concatenated with raw data

# Prefix

One bytes prefix to raw data.

|   Prefix         | Value  |
|------------------|--------|
| DaoBulla         | 1      |
| DaoProposalBulla | 2      |
| Coin             | 3      |
| Nullifier        | 4      |
| TokenID          | 5      |
| ContractID       | 6      |
| SecretKey        | 7      |
| PublicKey        | 8      |

# Raw data

`pallas::Base` field element of 32 bytes length.


# Implementation

Add wif encoding, decoding in `fp_from_bs58`, and `fp_to_bs58`.
