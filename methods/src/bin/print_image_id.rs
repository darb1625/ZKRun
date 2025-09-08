use hex::ToHex;

fn main() {
    // Read generated methods.rs as text and parse IMAGE_ID words
    let src: &str = include_str!(concat!(env!("OUT_DIR"), "/methods.rs"));
    // Prefer GUEST_ID (current risczero build), else IMAGE_ID, else generic ID
    let needle = if src.contains("GUEST_ID") {
        "GUEST_ID"
    } else if src.contains("IMAGE_ID") {
        "IMAGE_ID"
    } else {
        "ID"
    };
    let pos = src.find(needle).expect("ID not found in generated methods");
    // Prefer the array brackets after the '=' (skip the type brackets like [u32; 8])
    let eq_rel = src[pos..].find('=').expect("expected '=' after ID");
    let eq_idx = pos + eq_rel;
    let bracket_start = src[eq_idx..].find('[').expect("expected '[' after '='") + eq_idx;
    let bracket_end = src[bracket_start..].find(']').expect("expected ']' after '['") + bracket_start;
    let inside = &src[bracket_start + 1..bracket_end];
    fn parse_num(token: &str) -> Option<u32> {
        let mut s = token;
        if let Some(ix) = s.find("//") { s = &s[..ix]; }
        if let Some(ix) = s.find("/*") { s = &s[..ix]; }
        let filtered: String = s.chars().filter(|c| !c.is_whitespace() && *c != '_').collect();
        let s = filtered.as_str();
        if s.starts_with("0x") || s.starts_with("0X") {
            let hex: String = s[2..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            if hex.is_empty() { return None; }
            u32::from_str_radix(&hex, 16).ok()
        } else {
            let dec: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            if dec.is_empty() { return None; }
            dec.parse::<u32>().ok()
        }
    }
    let mut words_vec: Vec<u32> = Vec::new();
    for part in inside.split(',') {
        if let Some(val) = parse_num(part) { words_vec.push(val); }
        if words_vec.len() == 8 { break; }
    }
    assert!(words_vec.len() == 8, "Failed to parse 8 IMAGE_ID words");
    let mut words: [u32; 8] = [0; 8];
    for i in 0..8 { words[i] = words_vec[i]; }
    let mut bytes = [0u8; 32];
    for (i, word) in words.iter().enumerate() {
        let off = i * 4;
        bytes[off..off + 4].copy_from_slice(&word.to_be_bytes());
    }
    let hex = bytes.encode_hex::<String>();
    println!("IMAGE_ID=0x{}", hex);
}


