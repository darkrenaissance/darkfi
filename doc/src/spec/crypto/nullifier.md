# Nullifier

It's [commitment](commitment.md) to [coin](../payment/coin.md), or [bulla](../dao/bulla.md)'s [secret key](keypair.md), and serial number: [commit](commitment.md)(sk||sn)
each [coin](../payment/coin.md), [bulla](../dao/bulla.md) has unique private secret key, and unique serial, preventing double spending can be implemented through validation that nullifier have never been seen, proof that nullifer isn't included under current nullifiers [sparse merkle tree](merkletree.md) root.
