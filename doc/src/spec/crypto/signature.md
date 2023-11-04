# Signature
Signature for keypair over the elliptic curve, it's security hinged on the security of hash random oracle.

# Signature protocol
Signature = sign(sk, msg), `sk` private key used for message signature generation, `msg` message to be signed, signature as response, and challenge pair. to verify call verify(pk, msg, signature) with `pk` public key corresponding to message signing private key,  validate signature is valid for given msg and signature public key.

# Nonce leakage
Make sure the nonce or source of randomness is true random every time call to signature sign is called with the same keypair, otherwise the secret key can be leaked given just two signatures, $response_1 - response_2  = mask - sk * challenge_1 - mask + sk * challenge_2 = sk * (challenge_2 - challenge_1)$
