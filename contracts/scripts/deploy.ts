import { ethers } from "hardhat";

async function main() {
  const verifierAddress = process.env.RISC0_VERIFIER_ADDRESS;
  if (!verifierAddress) {
    throw new Error("Set RISC0_VERIFIER_ADDRESS to the deployed RISC Zero verifier address");
  }
  const RunVerifier = await ethers.getContractFactory("RunVerifier");
  const runVerifier = await RunVerifier.deploy(verifierAddress);
  await runVerifier.waitForDeployment();
  console.log("RunVerifier deployed to:", await runVerifier.getAddress());
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});


