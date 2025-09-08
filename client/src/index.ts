import 'dotenv/config';
import { ethers } from 'ethers';
import { createHash } from 'crypto';
import { encode } from 'cbor-x';

type Sample = { t: number; lat_microdeg: number; lon_microdeg: number };

function toBytes(hex: string): Uint8Array {
  const clean = hex.startsWith('0x') ? hex.slice(2) : hex;
  return Uint8Array.from(Buffer.from(clean, 'hex'));
}

function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) { out.set(p, off); off += p.length; }
  return out;
}

function sha256(data: Uint8Array): Uint8Array {
  const h = createHash('sha256');
  h.update(Buffer.from(data));
  return Uint8Array.from(h.digest());
}

function simulateRun(): { gps: Sample[]; start: number; end: number; totalMeters: number } {
  // Straight line ~5.1 km, 1 m/s, 1s intervals to satisfy max 12 m/s
  const startTime = Math.floor(Date.now() / 1000);
  const durationSec = 5200; // ~5.2 km at 1 m/s
  const samples: Sample[] = [];
  // Start near a fixed coordinate
  const startLat = 37.7749; // SF
  const startLon = -122.4194;
  const metersPerDegLat = 111_320;
  const metersPerDegLon = Math.cos((startLat * Math.PI) / 180) * 111_320;
  const metersStep = 1; // move east by 1 m per sample (speed 1 m/s)
  const degLonStep = metersStep / metersPerDegLon; // degrees per second
  const latMicro = Math.round(startLat * 1e6);
  for (let i = 0; i <= durationSec; i++) {
    const t = startTime + i;
    const lon = startLon + degLonStep * i;
    samples.push({ t, lat_microdeg: latMicro, lon_microdeg: Math.round(lon * 1e6) });
  }
  const endTime = startTime + durationSec;
  return { gps: samples, start: startTime, end: endTime, totalMeters: durationSec * metersStep };
}

function buildBlob(payload: any): Uint8Array {
  // Blob can include metadata to bind the run (kept private); encode as CBOR
  return encode(payload);
}

async function signSha256Blob(privKey: string, blob: Uint8Array): Promise<Uint8Array> {
  const digest = sha256(blob);
  const key = new ethers.SigningKey(privKey);
  const sig = key.sign(digest);
  const r = toBytes(sig.r);
  const s = toBytes(sig.s);
  const v = new Uint8Array([sig.v]); // v in {27,28}
  return concatBytes(r, s, v);
}

function encodeGuestInput(run: { gps: Sample[]; start: number; end: number }, blob: Uint8Array, sig: Uint8Array, pubkey: Uint8Array, maxElapsedSec: number, maxSpeedMps: number): Uint8Array {
  // Guest expects a CBOR map with numeric keys per struct tags
  const gpsList = run.gps.map(s => new Map<number, number>([[0, s.t], [1, s.lat_microdeg], [2, s.lon_microdeg]]));
  const m = new Map<number, any>([
    [0, gpsList],
    [1, run.start],
    [2, run.end],
    [3, maxElapsedSec],
    [4, maxSpeedMps],
    [5, blob],
    [6, sig],
    [7, pubkey],
  ]);
  return encode(m);
}

async function proveWithBonsai(_methodIdHex: string, _input: Uint8Array): Promise<{ journal: Uint8Array; seal: Uint8Array }> {
  // Placeholder: integrate @risc0/bonsai-sdk or REST when available
  throw new Error('Bonsai proving not configured in this example. Provide implementation to obtain {journal, seal}.');
}

async function submitOnChain(blobHash: Uint8Array, elapsedSec: number, journal: Uint8Array, seal: Uint8Array) {
  const rpcUrl = process.env.RPC_URL as string;
  const privKey = process.env.PRIVATE_KEY as string;
  const contractAddress = process.env.CONTRACT_ADDRESS as string;
  if (!rpcUrl || !privKey || !contractAddress) throw new Error('RPC_URL, PRIVATE_KEY, CONTRACT_ADDRESS required');
  const provider = new ethers.JsonRpcProvider(rpcUrl);
  const wallet = new ethers.Wallet(privKey, provider);
  const abi = [
    'function submitRun(bytes32 blobHash, uint32 elapsedSec, bytes journal, bytes seal) external',
    'event RunAccepted(address indexed player, bytes32 blobHash, uint32 elapsedSec)'
  ];
  const contract = new ethers.Contract(contractAddress, abi, wallet);
  const blobHashHex = '0x' + Buffer.from(blobHash).toString('hex');
  const tx = await contract.submitRun(blobHashHex, elapsedSec, journal, seal);
  console.log('Submitted tx:', tx.hash);
  const rcpt = await tx.wait();
  console.log('Mined in block', rcpt.blockNumber);
}

async function main() {
  const privKey = process.env.PRIVATE_KEY as string;
  const methodIdHex = process.env.METHOD_ID as string; // from guest IMAGE_ID
  const maxElapsedMinutes = Number(process.env.MAX_ELAPSED_MIN || 120);
  const maxSpeedMps = 12;
  if (!privKey) throw new Error('PRIVATE_KEY required');
  if (!methodIdHex) console.warn('METHOD_ID not set; proving will fail unless stubbed');

  // 1) Simulate GPS run
  const run = simulateRun();
  const elapsedSec = run.end - run.start;

  // 2) Build private blob
  const blob = buildBlob({
    note: 'ZKRun private run blob',
    created_at: Math.floor(Date.now() / 1000),
    start: run.start,
    end: run.end,
    nonce: ethers.hexlify(ethers.randomBytes(16)),
  });

  // 3) Sign SHA-256(blob) with Ethereum key (raw secp256k1)
  const sig = await signSha256Blob(privKey, blob);
  const pubkey = Uint8Array.from(Buffer.from(new ethers.SigningKey(privKey).publicKey.slice(2), 'hex'));

  // 4) Encode guest input as CBOR
  const input = encodeGuestInput(run, blob, sig, pubkey, maxElapsedMinutes * 60, maxSpeedMps);

  // 5) Prove with Bonsai (or other prover) to get receipt
  const { journal, seal } = await proveWithBonsai(methodIdHex, input);

  // 6) Submit on-chain minimal outputs
  const blobHash = sha256(blob);
  await submitOnChain(blobHash, elapsedSec, journal, seal);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});


