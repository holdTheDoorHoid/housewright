//! Encrypted, signed batches of operations: what a device pushes.
//!
//! A batch is several operations from one device, encrypted together:
//!
//! * header (plaintext, the server needs it): format version, space,
//!   epoch, a random 128-bit batch id, the device id, the device's own
//!   sequence number for the batch, the hash of the device's previous
//!   batch (`prev`: each device's batches form a hash chain, so a removed
//!   device cannot slip in new batches under old numbers; D-99), the
//!   largest operation clock in the batch (`max_wall_ms`, used by the clock
//!   guard, D-93), and the nonce.
//! * `aad = transcript("housewright/v1/batch", every header field)`.
//! * `ct = XChaCha20-Poly1305(ops key of the epoch, nonce, pad(plaintext,
//!   256), aad)`, so a batch cannot be moved to another household, epoch,
//!   device or sequence number without failing to decrypt.
//! * `sig = Ed25519(device, transcript("housewright/v1/batch-sig", aad, ct))`.
//!   The server checks it too (so a device cannot push in another's name);
//!   every receiver checks it against the roster.

use serde::{Deserialize, Serialize};

use crate::encoding::{hex, unhex_array};
use crate::keys::{Purpose, b64_64, b64_vec};
use crate::primitives::sha256;
use crate::primitives::{
    NONCE_LEN, open_with_nonce, pad, random, seal_with_nonce, transcript, unpad,
};
use crate::{CryptoError, DeviceId, DevicePublic, DeviceSecret, KeyChain, Result, SpaceId};

pub const BATCH_PAD: usize = 256;
/// The largest ciphertext a batch may have (D-100). Clients keep batches
/// well under it; the server refuses anything bigger.
pub const MAX_BATCH_CT: usize = 1024 * 1024;
/// The hash a device's first batch names as `prev`.
pub const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchHeader {
    pub v: u8,
    pub space: SpaceId,
    pub epoch: u32,
    /// 16 random bytes, hex. The server deduplicates retries by it.
    pub batch_id: String,
    pub device: DeviceId,
    pub device_seq: u64,
    /// [`batch_hash`] of this device's previous batch (hex); 64 zeros for
    /// its first.
    pub prev: String,
    pub max_wall_ms: u64,
    #[serde(with = "b64_vec")]
    pub nonce: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedBatch {
    pub header: BatchHeader,
    #[serde(with = "b64_vec")]
    pub ct: Vec<u8>,
    #[serde(with = "b64_64")]
    pub sig: [u8; 64],
}

pub fn new_batch_id() -> Result<String> {
    Ok(hex(&random::<16>()?))
}

fn aad(h: &BatchHeader) -> Result<Vec<u8>> {
    let id: [u8; 16] = unhex_array(&h.batch_id)?;
    let prev: [u8; 32] = unhex_array(&h.prev)?;
    Ok(transcript(
        "housewright/v1/batch",
        &[
            &[h.v],
            &h.space.0,
            &h.epoch.to_be_bytes(),
            &id,
            &h.device.0,
            &h.device_seq.to_be_bytes(),
            &prev,
            &h.max_wall_ms.to_be_bytes(),
            &h.nonce,
        ],
    ))
}

fn sig_input(h: &BatchHeader, ct: &[u8]) -> Result<Vec<u8>> {
    Ok(transcript("housewright/v1/batch-sig", &[&aad(h)?, ct]))
}

/// `SHA-256(transcript("housewright/v1/batch-hash", aad, ct))`, hex: what
/// the next batch of the same device names as `prev`. The server can
/// compute it too (it has the header and the ciphertext).
pub fn batch_hash(b: &SealedBatch) -> Result<String> {
    Ok(hex(&sha256(&transcript(
        "housewright/v1/batch-hash",
        &[&aad(&b.header)?, &b.ct],
    ))))
}

/// Encrypt and sign `plaintext` under the chain's current epoch.
#[allow(clippy::too_many_arguments)]
pub fn seal_batch(
    device: &DeviceSecret,
    chain: &KeyChain,
    space: SpaceId,
    batch_id: String,
    device_seq: u64,
    prev: &str,
    max_wall_ms: u64,
    plaintext: &[u8],
) -> Result<SealedBatch> {
    let epoch = chain.current_epoch();
    let nonce = random::<NONCE_LEN>()?;
    let header = BatchHeader {
        v: 1,
        space,
        epoch,
        batch_id,
        device: device.id(),
        device_seq,
        prev: prev.to_string(),
        max_wall_ms,
        nonce: nonce.to_vec(),
    };
    let key = chain.key(space, epoch, Purpose::Ops)?;
    let ct = seal_with_nonce(&key, &nonce, &pad(plaintext, BATCH_PAD), &aad(&header)?)?;
    let sig = device.sign(&sig_input(&header, &ct)?);
    Ok(SealedBatch { header, ct, sig })
}

/// Check the signature only (the server does this; it has no keys).
pub fn verify_batch(b: &SealedBatch, author: &DevicePublic) -> Result<()> {
    if b.header.v != 1 {
        return Err(CryptoError::Rejected("unknown batch format".into()));
    }
    if author.id != b.header.device {
        return Err(CryptoError::Rejected("batch signer mismatch".into()));
    }
    if b.header.nonce.len() != NONCE_LEN {
        return Err(CryptoError::Malformed("bad nonce".into()));
    }
    if b.ct.len() > MAX_BATCH_CT {
        return Err(CryptoError::Rejected("batch too large".into()));
    }
    author.verify(&sig_input(&b.header, &b.ct)?, &b.sig)
}

/// Verify, then decrypt. `space` is the household the receiver belongs to:
/// a batch from any other household is refused before decryption.
pub fn open_batch(
    b: &SealedBatch,
    space: SpaceId,
    chain: &KeyChain,
    author: &DevicePublic,
) -> Result<Vec<u8>> {
    if b.header.space != space {
        return Err(CryptoError::Rejected("batch from another household".into()));
    }
    verify_batch(b, author)?;
    let key = chain.key(space, b.header.epoch, Purpose::Ops)?;
    let nonce: [u8; NONCE_LEN] = b
        .header
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| CryptoError::Decrypt)?;
    let padded = open_with_nonce(&key, &nonce, &b.ct, &aad(&b.header)?)?;
    unpad(&padded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (DeviceSecret, KeyChain, SpaceId, SealedBatch) {
        let d = DeviceSecret::generate().unwrap();
        let c = KeyChain::generate().unwrap();
        let s = SpaceId::random().unwrap();
        let b = seal_batch(
            &d,
            &c,
            s,
            new_batch_id().unwrap(),
            1,
            ZERO_HASH,
            1000,
            b"[ops]",
        )
        .unwrap();
        (d, c, s, b)
    }

    #[test]
    fn round_trip() {
        let (d, c, s, b) = setup();
        assert_eq!(open_batch(&b, s, &c, &d.public()).unwrap(), b"[ops]");
        assert_eq!(b.ct.len(), BATCH_PAD + 16, "padded");
    }

    #[test]
    fn tampering_is_rejected() {
        let (d, c, s, b) = setup();
        let p = d.public();
        let mut t = b.clone();
        t.ct[3] ^= 0x40;
        assert_eq!(open_batch(&t, s, &c, &p), Err(CryptoError::Signature));
        let mut t = b.clone();
        t.header.device_seq = 2;
        assert_eq!(open_batch(&t, s, &c, &p), Err(CryptoError::Signature));
        let mut t = b.clone();
        t.sig[0] ^= 1;
        assert_eq!(open_batch(&t, s, &c, &p), Err(CryptoError::Signature));
        // The chain link is signed and authenticated too.
        let mut t = b.clone();
        t.header.prev = "11".repeat(32);
        assert_eq!(open_batch(&t, s, &c, &p), Err(CryptoError::Signature));
        t.sig = d.sign(&sig_input(&t.header, &t.ct).unwrap());
        assert_eq!(open_batch(&t, s, &c, &p), Err(CryptoError::Decrypt));
        assert_ne!(batch_hash(&t).unwrap(), batch_hash(&b).unwrap());
        // Another device's key.
        let other = DeviceSecret::generate().unwrap();
        assert!(open_batch(&b, s, &c, &other.public()).is_err());
    }

    #[test]
    fn moved_between_households_or_epochs_is_rejected() {
        let (d, c, s, b) = setup();
        let p = d.public();
        // Same bytes presented to another household.
        let s2 = SpaceId::random().unwrap();
        assert!(open_batch(&b, s2, &c, &p).is_err());
        // An insider (holds the keys and a device key) re-signs the same
        // ciphertext with a changed header: the AEAD still refuses.
        let mut t = b.clone();
        t.header.space = s2;
        t.sig = d.sign(&sig_input(&t.header, &t.ct).unwrap());
        assert_eq!(open_batch(&t, s2, &c, &p), Err(CryptoError::Decrypt));
        let c2 = c.rotated().unwrap();
        let mut t = b.clone();
        t.header.epoch = 1;
        t.sig = d.sign(&sig_input(&t.header, &t.ct).unwrap());
        assert_eq!(open_batch(&t, s, &c2, &p), Err(CryptoError::Decrypt));
        // A receiver without the epoch's key cannot read it.
        let later = seal_batch(&d, &c2, s, new_batch_id().unwrap(), 2, ZERO_HASH, 1, b"x").unwrap();
        assert!(open_batch(&later, s, &c, &p).is_err());
    }
}
