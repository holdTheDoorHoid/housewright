//! The template library embedded at build time (see `build.rs`).

include!(concat!(env!("OUT_DIR"), "/bundled_templates.rs"));

/// The vendored appliance library as a gzip-compressed bundle (D-147).
pub static LIBRARY_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/library.json.gz"));
