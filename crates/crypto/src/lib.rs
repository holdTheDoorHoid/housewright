//! Housewright's end-to-end encryption (D-14, D-15, D-89 to D-93).
//!
//! Everything a sync server stores is built here: device identities, the
//! household key chain, key envelopes, encrypted operation batches,
//! encrypted files, the signed device roster, signed requests and the
//! pairing exchange. The design, written for a reviewer, is in
//! `docs/SECURITY.md`; every byte layout below is described there.
//!
//! Only mainstream audited-family primitives are used, never home-made
//! ones: Ed25519 (`ed25519-dalek`), HPKE RFC 9180 with X25519, HKDF-SHA256
//! and ChaCha20-Poly1305 (`hpke`), XChaCha20-Poly1305 (`chacha20poly1305`),
//! HKDF and HMAC with SHA-256 (`hkdf`, `hmac`, `sha2`).
//!
//! Licensed AGPL-3.0-only.

pub mod batch;
pub mod blob;
pub mod encoding;
pub mod keys;
pub mod pairing;
pub mod primitives;
pub mod recovery;
pub mod request;
pub mod roster;
pub mod wire;
pub mod wrap;

pub use keys::{DeviceId, DevicePublic, DeviceSecret, KeyChain, SpaceId};

/// Every way a cryptographic check can fail. Messages are deliberately
/// short and never contain key material or plaintext.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    #[error("the data could not be decrypted (wrong key, or it was changed)")]
    Decrypt,
    #[error("a signature did not verify")]
    Signature,
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("the system's random number generator failed")]
    Rng,
}

pub type Result<T> = std::result::Result<T, CryptoError>;

/// How far ahead of the server's clock an operation's hybrid logical clock
/// may be (D-93). Also the replay window for signed requests.
pub const MAX_CLOCK_AHEAD_MS: u64 = 5 * 60 * 1000;
