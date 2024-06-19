# darkpool for drk

## renegade for dummies

- renegade[^1] implemented for erc20 transparent token to be swapped between two parties, each party create a wallet $W = (B,O,F,K,r)$ of balance, order, fee, public keys, blinding value with transition state for every internal (within-darkpool) $T_I=(\tilde{m_I},\tilde{v_i})$ encrypted by the receiver key, or external transaction (transparent) $T_E=(\tilde{m_E},\tilde{v_E},\tilde{d_E})$, each trader picks order o to be matched, it's corresponding covering balance b, and fee f, blinding r, and create commitments to o, b, f, r as $H_o$, $H_b$, $H_f$, $H_r$ and a VALID-COMMITMENT zk-snark proof that those commitments are derived from known private witnesses.
- from on-chain published VALID-COMMITMENT counterparties validate it's valid, and start matching process.
- reconstruct commitment over MPC through secret shares [o], [b], [f], [r], first construct match tuple $M=(\tilde{m_1}, \tilde{m_2}, \tilde{v_1}, \tilde{v_2}, d, f_1, f_2)$ validate exchange coins match, $v_1==v_2$, directions, and that fee covers relay fee. reconstruct commitments shares $H_{o_1}$, $H_{b_1}$, $H_{f_1}$, $H_{o_2}$, $H_{b_2}$, $H_{f_2}$, open shares through third party, then exchange notes.

## renegade performance
- both bulletproof over mpc, and collaborative zksnark over mpc are 2 times proving, and verifying of single prover proof [^10],[^11]

## dark renegade

- internal, and external wallet update state $T_I$, $T_E$ are replaced by drk money transfer contract.
- eliminate third-party for opening shares using witness encryption.

# framework

- mpc-stark[^2] is spdz mpc built over stark-curve (curve can change), we have sage implementation [^12]
- mpc-bulletproof[^3] mpc r1cs with mpc inner product proof built over mpc-stark, we have sage implementation of the ipp [^13]
- plug darkfi-p2p network into mpc-bulletproof
- fork renegade[^4]
  - if the aim is to use halo2 proof zk-proofs, fork mpc-stark, and replace stark-curve with pallas-curve.
  - rewrite renegade zk-snark proofs with darkfi compiler
  - replace wallet update with money transfer contracts

## open fairness
   - opening shares between two strategic players in p2p network without a third-party is impossible [^5].
   - one turn around is witness encrypting (WE) [^6],[^9],[^8]  the match shares  for which the other party encrypt with witness (the other share), and the opposite for the peer [^7]

[^1]: https://renegade.fi/whitepaper.pdf
[^2]: https://github.com/renegade-fi/mpc-stark
[^3]: https://github.com/renegade-fi/mpc-bulletproof
[^4]: https://github.com/renegade-fi/renegade
[^5]: https://kodu.ut.ee/~swen/courses/crypto-ii/2008/cleve1986.pdf
[^6]: https://eprint.iacr.org/2013/258.pdf
[^7]: https://eprint.iacr.org/2017/1091.pdf
[^8]: https://github.com/guberti/witness-encryption-demos
[^9]: https://arxiv.org/pdf/2112.04581.pdf
[^10]: https://eprint.iacr.org/2021/1530
[^11]: https://github.com/renegade-fi/mpc-bulletproof/pull/14
[^12]: https://codeberg.org/darkrenaissance/darkfi/src/branch/master/script/research/mpc
[^13]: https://codeberg.org/darkrenaissance/darkfi/src/branch/master/script/research/bulletproof-mpc
