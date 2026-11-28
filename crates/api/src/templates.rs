//! The template library embedded at build time (see `build.rs`).

include!(concat!(env!("OUT_DIR"), "/bundled_templates.rs"));
