use hex::ToHex;

fn main() {
    // Access the generated module from embed_methods. Depending on tooling, the module
    // may be named after the package + method (zkrun_methods_guest).
    let image_id_words: [u32; 8] = zkrun_methods::zkrun_methods_guest::IMAGE_ID;
    // Convert to bytes big-endian
    let mut bytes = [0u8; 32];
    for (i, word) in image_id_words.iter().enumerate() {
        let off = i * 4;
        bytes[off..off + 4].copy_from_slice(&word.to_be_bytes());
    }
    let hex = bytes.encode_hex::<String>();
    println!("IMAGE_ID=0x{}", hex);
}


