// SPDX-License-Identifier: LGPLv3
// Implemention based on Vitalik's idea:
// https://ethresear.ch/t/you-can-kinda-abuse-ecrecover-to-do-ecmul-in-secp256k1-today

pragma solidity ^0.8.20;

contract Secp256k1 {
    // solhint-disable-next-line
    uint256 private constant gx =
        0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798;
    // solhint-disable-next-line
    uint256 private constant m = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141;

    // mulVerify returns true if `Q = s * G` on the secp256k1 curve
    // qKeccak is defined as uint256(keccak256(abi.encodePacked(qx, qy))
    function mulVerify(uint256 scalar, uint256 qKeccak) public pure returns (bool) {
        address qRes = ecrecover(0, 27, bytes32(gx), bytes32(mulmod(scalar, gx, m)));
        return uint160(qKeccak) == uint160(qRes);
    }
}
