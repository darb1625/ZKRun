#![no_main]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use k256::ecdsa::Signature as EcdsaSignature;
use k256::ecdsa::{signature::DigestVerifier, VerifyingKey};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use minicbor::Decode;
use risc0_zkvm::guest::env;
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};

// Input types (decoded via CBOR)

#[derive(Debug, Decode)]
struct Sample {
    #[n(0)]
    t: u64, // seconds
    #[n(1)]
    lat_microdeg: i32, // degrees * 1e6
    #[n(2)]
    lon_microdeg: i32, // degrees * 1e6
}

#[derive(Debug, Decode)]
struct RunInput {
    #[n(0)]
    gps: Vec<Sample>,
    #[n(1)]
    start_time: u64,
    #[n(2)]
    end_time: u64,
    #[n(3)]
    max_elapsed_sec: u32,
    #[n(4)]
    max_speed_mps: u32, // 12
    #[n(5)]
    blob: Vec<u8>,
    #[n(6)]
    sig: Vec<u8>, // 65 bytes r||s||v
    #[n(7)]
    pubkey: Vec<u8>, // 65-byte uncompressed SEC1 (0x04 || X || Y)
}

const EARTH_RADIUS_M: i64 = 6_371_000; // meters
const Q: i128 = 1_i128 << 32; // Q32.32 fixed-point scale

fn deg_to_rad_q32(deg_micro: i32) -> i128 {
    // radians = deg * pi / 180
    // deg_micro is degrees * 1e6
    let deg_num: i128 = deg_micro as i128;
    // pi in Q32.32
    let pi_q32: i128 = (core::f64::consts::PI * (Q as f64)) as i128;
    // Convert degrees (micro) to Q32.32
    let deg_q32: i128 = (deg_num * (Q as i128)) / 1_000_000_i128;
    // Multiply by pi/180
    (deg_q32 * pi_q32) / ((180_i128) * (Q as i128))
}

fn wrap_pi_q32(mut x: i128) -> i128 {
    // wrap to [-pi, pi]
    let two_pi = (core::f64::consts::PI * 2.0 * (Q as f64)) as i128;
    let pi = (core::f64::consts::PI * (Q as f64)) as i128;
    while x > pi { x -= two_pi; }
    while x < -pi { x += two_pi; }
    x
}

fn q32_mul(a: i128, b: i128) -> i128 { (a * b) / Q }
fn q32_div(a: i128, b: i128) -> i128 { (a * Q) / b }

fn q32_cos(x: i128) -> i128 {
    // Cosine polynomial approximation around 0: 1 - x^2/2 + x^4/24 - x^6/720
    let x = wrap_pi_q32(x);
    let x2 = q32_mul(x, x);
    let x4 = q32_mul(x2, x2);
    let x6 = q32_mul(x4, x2);
    let term1 = Q; // 1.0
    let term2 = -x2 / 2;
    let term3 = x4 / 24;
    let term4 = -x6 / 720;
    term1 + term2 + term3 + term4
}

fn isqrt_u128(x: u128) -> u128 {
    // Integer sqrt via binary method
    if x == 0 { return 0; }
    let mut r: u128 = 0;
    let mut bit: u128 = 1_u128 << 126; // highest even power of two <= x
    while bit > x { bit >>= 2; }
    let mut n = x;
    while bit != 0 {
        if n >= r + bit { n -= r + bit; r = (r >> 1) + bit; }
        else { r >>= 1; }
        bit >>= 2;
    }
    r
}

fn distance_segment_meters(lat1: i32, lon1: i32, lat2: i32, lon2: i32) -> u64 {
    // Equirectangular approximation: x = dlon * cos(lat_avg), y = dlat, d = R * sqrt(x^2 + y^2)
    let phi1 = deg_to_rad_q32(lat1);
    let phi2 = deg_to_rad_q32(lat2);
    let lam1 = deg_to_rad_q32(lon1);
    let lam2 = deg_to_rad_q32(lon2);
    let dphi = phi2 - phi1; // Q32.32
    let dlam = lam2 - lam1; // Q32.32
    let lat_avg = (phi1 + phi2) / 2;
    let cos_lat = q32_cos(lat_avg); // Q32.32
    let x = q32_mul(dlam, cos_lat); // Q32.32
    let y = dphi; // Q32.32
    // sqrt(x^2 + y^2) in Q32.32 -> scale to meters by multiplying R and dividing by Q
    let x2 = q32_mul(x, x) as i128; // Q32.32
    let y2 = q32_mul(y, y) as i128; // Q32.32
    let sum = (x2 + y2) as i128; // Q32.32
    // Convert to u128 for sqrt. Scale to Q32.32 squared -> Q32.32
    let sum_u = if sum < 0 { 0_u128 } else { sum as u128 };
    let root_q32 = isqrt_u128(sum_u * (Q as u128)); // sqrt in Q32.32 (by scaling once)
    let meters = ((EARTH_RADIUS_M as i128) * (root_q32 as i128)) / (Q as i128);
    if meters < 0 { 0 } else { meters as u64 }
}

fn verify_signature(blob: &[u8], sig: &[u8], pubkey: &[u8]) -> Option<[u8; 20]> {
    if sig.len() != 65 { return None; }
    if pubkey.len() != 65 { return None; }
    // Compute SHA-256(blob)
    let mut hasher = Sha256::new();
    hasher.update(blob);
    let digest = hasher.finalize();
    // Parse signature r||s||v (ignore v)
    let mut sig64 = [0u8; 64];
    sig64.copy_from_slice(&sig[0..64]);
    let signature = EcdsaSignature::from_slice(&sig64).ok()?;
    // Parse provided uncompressed public key
    let verify_key = VerifyingKey::from_sec1_bytes(pubkey).ok()?;
    // Verify digest
    if verify_key.verify_digest(digest.into(), &signature).is_err() {
        return None;
    }
    // Ethereum address = last 20 bytes of keccak256(uncompressed_pubkey[1..])
    let mut keccak = Keccak::v256();
    let mut out = [0u8; 32];
    keccak.update(&pubkey[1..]);
    keccak.finalize(&mut out);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&out[12..]);
    Some(addr)
}

fn main() {
    // Read CBOR-encoded input
    let input: Vec<u8> = env::read();
    let run_in: RunInput = match minicbor::decode(&input) {
        Ok(v) => v,
        Err(_) => {
            env::commit_slice(&[0u8]);
            return;
        }
    };

    // Basic checks
    if run_in.gps.len() < 2 {
        env::commit_slice(&[0u8]);
        return;
    }
    if !(run_in.start_time <= run_in.end_time) {
        env::commit_slice(&[0u8]);
        return;
    }

    // Signature check (and recompute blob hash)
    let signer_addr = match verify_signature(&run_in.blob, &run_in.sig, &run_in.pubkey) {
        Some(a) => a,
        None => { env::commit_slice(&[0u8]); return; }
    };
    // Compute blob hash (SHA-256)
    let mut hasher = Sha256::new();
    hasher.update(&run_in.blob);
    let blob_hash = hasher.finalize();

    // Walk samples
    let mut total_distance_m: u64 = 0;
    let mut last_t = run_in.gps[0].t;
    for w in run_in.gps.windows(2) {
        let a = &w[0];
        let b = &w[1];
        if !(b.t > a.t) { env::commit_slice(&[0u8]); return; }
        let dt = b.t - a.t;
        let d = distance_segment_meters(a.lat_microdeg, a.lon_microdeg, b.lat_microdeg, b.lon_microdeg);
        total_distance_m = total_distance_m.saturating_add(d);
        if dt == 0 { env::commit_slice(&[0u8]); return; }
        // max speed check
        let speed_mps = d / dt as u64; // floor
        if speed_mps > (run_in.max_speed_mps as u64) {
            env::commit_slice(&[0u8]);
            return;
        }
        last_t = b.t;
    }

    let elapsed = last_t - run_in.gps[0].t;
    if elapsed > (run_in.max_elapsed_sec as u64) {
        env::commit_slice(&[0u8]);
        return;
    }
    if total_distance_m < 5_000 {
        env::commit_slice(&[0u8]);
        return;
    }

    // Build journal: [passed=1][elapsed_sec u32 BE][blob_hash 32][signer_addr 20]
    let mut journal: Vec<u8> = Vec::with_capacity(1 + 4 + 32 + 20);
    journal.push(1u8);
    let elapsed_u32 = if elapsed > u32::MAX as u64 { u32::MAX } else { elapsed as u32 };
    journal.extend_from_slice(&elapsed_u32.to_be_bytes());
    journal.extend_from_slice(&blob_hash);
    journal.extend_from_slice(&signer_addr);
    env::commit_slice(&journal);
}

risc0_zkvm::guest::entry!(main);


