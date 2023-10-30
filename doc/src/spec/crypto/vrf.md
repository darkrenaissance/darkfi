# ECVRF

ecvrf[^1] is elliptic curve verifiable random function that satisfies:

- trusted uniqueness [^2]
- trusted collision resistance [^3]
- full pseudo-randomness [^4]

## ECVRF protocol

### Proof generation

$proof = prove(sk, data)$, `sk` is VRF private key, `data` is input data as stream of bytes, proof is the vrf output.
generate a vrf proof, that can be publicly verified.

### Proof verification
$verify(pk, proof, data)$, `pk` is VRF public key, validate that the proof is correct.

[^1]: https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04#section-5
[^2]: https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04#section-3.1
[^3]: https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04#section-3.2
[^4]: https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04#section-3.3
