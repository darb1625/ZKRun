// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IRiscZeroVerifier} from "./IRiscZeroVerifier.sol";

contract RunVerifier {
    event RunAccepted(address indexed player, bytes32 blobHash, uint32 elapsedSec);

    IRiscZeroVerifier public immutable verifier;
    // Replace with the IMAGE_ID produced by building the RISC Zero guest method.
    bytes32 public constant IMAGE_ID = 0x0000000000000000000000000000000000000000000000000000000000000000;

    constructor(address verifierAddress) {
        verifier = IRiscZeroVerifier(verifierAddress);
    }

    // Journal layout: [1 byte passed][4 bytes elapsed_sec BE][32 bytes blob_hash][20 bytes signer_addr]
    function submitRun(bytes32 blobHash, uint32 elapsedSec, bytes calldata journal, bytes calldata seal) external {
        // Verify the ZK proof with the provided journal
        verifier.verify(seal, IMAGE_ID, journal);

        // Parse journal
        require(journal.length == 1 + 4 + 32 + 20, "bad journal");
        require(uint8(journal[0]) == 1, "not passed");

        uint32 elapsedFromJournal = _readU32BE(journal, 1);
        bytes32 blobFromJournal = _readBytes32(journal, 1 + 4);
        address signerFromJournal = _readAddress(journal, 1 + 4 + 32);

        require(elapsedFromJournal == elapsedSec, "elapsed mismatch");
        require(blobFromJournal == blobHash, "blob mismatch");
        require(signerFromJournal == msg.sender, "sender mismatch");

        emit RunAccepted(msg.sender, blobHash, elapsedSec);
    }

    function _readU32BE(bytes calldata data, uint256 offset) internal pure returns (uint32 v) {
        require(data.length >= offset + 4, "oob");
        v = (uint32(uint8(data[offset])) << 24)
            | (uint32(uint8(data[offset + 1])) << 16)
            | (uint32(uint8(data[offset + 2])) << 8)
            | (uint32(uint8(data[offset + 3])));
    }

    function _readBytes32(bytes calldata data, uint256 offset) internal pure returns (bytes32 v) {
        require(data.length >= offset + 32, "oob");
        assembly {
            v := calldataload(add(data.offset, offset))
        }
    }

    function _readAddress(bytes calldata data, uint256 offset) internal pure returns (address a) {
        require(data.length >= offset + 20, "oob");
        bytes32 word;
        assembly {
            word := calldataload(add(data.offset, offset))
        }
        a = address(uint160(uint256(word >> 96)));
    }
}


