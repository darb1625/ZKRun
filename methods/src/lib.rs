// This includes the generated `methods.rs` which exposes a module per guest crate
// e.g., `pub mod zkrun_guest { pub const IMAGE_ID: [u32; 8]; pub const ELF: &'static [u8]; }`
include!(concat!(env!("OUT_DIR"), "/methods.rs"));


