use hex::ToHex;

fn main() {
    // Read generated methods.rs as text and parse IMAGE_ID words
    let src: &str = include_str!(concat!(env!("OUT_DIR"), "/methods.rs"));
    let needle = "IMAGE_ID";
    let pos = src.find(needle).expect("IMAGE_ID not found in generated methods");
    let bracket_start = src[pos..].find('[').expect("[") + pos;
    let bracket_end = src[bracket_start..].find(']').expect("]") + bracket_start;
    let inside = &src[bracket_start + 1..bracket_end];
    let mut words: [u32; 8] = [0; 8];
    for (i, part) in inside.split(',').filter(|s| !s.trim().is_empty()).take(8).enumerate() {
        let token = part.trim();
        let val = if let Some(hexstr) = token.strip_prefix("0x") {
            u32::from_str_radix(hexstr.trim(), 16).expect("parse hex u32")
        } else {
            token.parse::<u32>().expect("parse dec u32")
        };
        words[i] = val;
    }
    let mut bytes = [0u8; 32];
    for (i, word) in words.iter().enumerate() {
        let off = i * 4;
        bytes[off..off + 4].copy_from_slice(&word.to_be_bytes());
    }
    let hex = bytes.encode_hex::<String>();
    println!("IMAGE_ID=0x{}", hex);
}


