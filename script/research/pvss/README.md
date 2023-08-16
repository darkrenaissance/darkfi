Publicly Verifiable Secret Sharing
==================================

## `dleq.sage`

A quick overview of the DLEQ sigma protocol, both interactive and
non-interactive.

## `pvss.sage`

This is an implementation of the paper found at
<https://www.win.tue.nl/~berry/papers/crypto99.pdf>.

With this scheme, there exists a trusted dealer which picks a secret
value, and creates shares of the secret using Shamir Secret Sharing
within a given threshold and a number of participants of the PVSS 
scheme.

Participants publish their public keys, and the dealer is able to
encrypt the shares to their public keys. The dealer shows that the
encrypted shares are consistent by producing a proof of knowledege
of the unique `p(i), 1 <= i <= n`, satisfying `X_i = g^p(i), Y_i = y_i^p(i)`.

These proofs can be verified by anyone.

The participants are able to decrypt their own shares, sample a set of
threshold `t` shares and reconstruct the secret value.

## `reed_solomon.sage`

Reed Solomon check in the context of elliptic curves, used in
`scrape.sage`.

## `scrape.sage`

Implementation of <https://eprint.iacr.org/2017/216.pdf>.
This is similar to Schoenmaker's PVSS (`pvss.sage`) but it requires
only a linear number of exponentiations for sharing, verifying, and
reconstruction, whereas the previous scheme has quadratic complexity.
