# signature
signature for keypair over the elliptic curve, with security hinged on the security of hash random oracle.

# signature protocol
signature = sign(sk, msg), `sk` private key used for message signature generation, `msg` message to be signed, signature as response, and challenge pair
verify(pk, msg, signature) `pk` public key corresponding to message signing private key,  validate signature is valid for given msg with signature public key.

# nonce leakage
make sure the nonce, or source of randomness is true random every time call to signature sign is called with the same keypair, otherwise the secret key be leaked given just two signatures, $response_1 - response_2  = mask - sk * challenge_1 - mask + sk * challenge_2 = sk * (challenge_2 - challenge_1)$
