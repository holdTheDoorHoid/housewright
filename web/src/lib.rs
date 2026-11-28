//! Native builds of this crate are empty; see `wasm.rs`.
#![cfg_attr(not(target_arch = "wasm32"), allow(unused))]
#[cfg(target_arch = "wasm32")]
mod wasm;
