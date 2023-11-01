# Token Id

Each token has unique [token id](token_id.md) derived as

$$ hash(PREFIX || key^{public}_x || key^{public}_y) $$

[$key^{public}$](../crypto/keypair.md) is [authority key, or public key](../crypto/keypair.md).
