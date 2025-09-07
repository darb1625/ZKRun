// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IRiscZeroVerifier {
    // Typical RISC Zero verifier signature: verify(seal, imageId, journal)
    function verify(bytes calldata seal, bytes32 imageId, bytes calldata journal) external view;
}


