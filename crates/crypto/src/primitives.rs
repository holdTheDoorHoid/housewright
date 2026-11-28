//! The building blocks every other module uses. Each one is a thin call
//! into an audited-family crate; nothing here invents a primitive.
//!
//! * [`transcript`]: unambiguous byte strings for signatures and AEAD
//!   associated data: a domain-separation label, then every part, each
//!   prefixed by its length as a 32-bit big-endian number. Two different
//!   lists of parts can never produce the same bytes.
//! * [`derive_key`]: HKDF-SHA256 (RFC 5869).
//! * [`seal`] / [`open`]: XChaCha20-Poly1305 with a random 192-bit nonce
//!   (random nonces are safe at this size; there is no counter to keep).
//! * [`pad`] / [`unpad`]: length hiding to a block size before encryption.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit as _, Mac};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{CryptoError, Result};

pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;

/// Fill an array from the operating system's random number generator.
pub fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| CryptoError::Rng)?;
    Ok(b)
}

/// A secret array from the system RNG, wiped from memory when dropped.
pub fn random_secret<const N: usize>() -> Result<Zeroizing<[u8; N]>> {
    let mut b = Zeroizing::new([0u8; N]);
    getrandom::fill(b.as_mut()).map_err(|_| CryptoError::Rng)?;
    Ok(b)
}

/// `u32be(len(label)) || label || u32be(len(p1)) || p1 || ...`
pub fn transcript(label: &str, parts: &[&[u8]]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(4 + label.len() + parts.iter().map(|p| p.len() + 4).sum::<usize>());
    out.extend_from_slice(&(label.len() as u32).to_be_bytes());
    out.extend_from_slice(label.as_bytes());
    for p in parts {
        out.extend_from_slice(&(p.len() as u32).to_be_bytes());
        out.extend_from_slice(p);
    }
    out
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// HKDF-SHA256 extract-and-expand to one 32-byte key.
pub fn derive_key(ikm: &[u8], salt: &[u8], info: &[u8]) -> Zeroizing<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = Zeroizing::new([0u8; 32]);
    hk.expand(info, okm.as_mut())
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    okm
}

/// HMAC-SHA256.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key)
        .expect("HMAC accepts keys of any length");
    m.update(data);
    m.finalize().into_bytes().into()
}

fn cipher(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

/// XChaCha20-Poly1305 with a caller-chosen nonce. Callers use a fresh random
/// nonce every time.
pub fn seal_with_nonce(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    cipher(key)
        .encrypt(
            &XNonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Malformed("message too long to encrypt".into()))
}

pub fn open_with_nonce(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    cipher(key)
        .decrypt(
            &XNonce::from(*nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Decrypt)
}

/// `nonce || ciphertext || tag`, with a fresh random nonce.
pub fn seal(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let nonce = random::<NONCE_LEN>()?;
    let mut out = nonce.to_vec();
    out.extend(seal_with_nonce(key, &nonce, plaintext, aad)?);
    Ok(out)
}

pub fn open(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::Decrypt);
    }
    let (nonce, ct) = data.split_at(NONCE_LEN);
    let nonce: &[u8; NONCE_LEN] = nonce.try_into().expect("split at NONCE_LEN");
    open_with_nonce(key, nonce, ct, aad)
}

/// `u32be(len) || data || zeros`, rounded up to a multiple of `block`, so
/// the server learns sizes only to the nearest block.
pub fn pad(data: &[u8], block: usize) -> Vec<u8> {
    let raw = data.len() + 4;
    let total = raw.div_ceil(block) * block;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
    out.resize(total, 0);
    out
}

pub fn unpad(padded: &[u8]) -> Result<Vec<u8>> {
    let bad = || CryptoError::Malformed("bad padding".into());
    let len_bytes: [u8; 4] = padded
        .get(..4)
        .ok_or_else(bad)?
        .try_into()
        .map_err(|_| bad())?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    let body = padded.get(4..4 + len).ok_or_else(bad)?;
    Ok(body.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_is_unambiguous() {
        assert_ne!(
            transcript("x", &[b"ab", b"c"]),
            transcript("x", &[b"a", b"bc"])
        );
        assert_ne!(transcript("x", &[b"a"]), transcript("y", &[b"a"]));
    }

    #[test]
    fn seal_open_and_tamper() {
        let k = [7u8; 32];
        let ct = seal(&k, b"secret", b"ad").unwrap();
        assert_eq!(open(&k, &ct, b"ad").unwrap(), b"secret");
        assert_eq!(open(&k, &ct, b"other ad"), Err(CryptoError::Decrypt));
        let mut bad = ct.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert_eq!(open(&k, &bad, b"ad"), Err(CryptoError::Decrypt));
        assert_eq!(open(&[8u8; 32], &ct, b"ad"), Err(CryptoError::Decrypt));
        assert_ne!(
            seal(&k, b"secret", b"ad").unwrap(),
            ct,
            "fresh nonce each time"
        );
    }

    #[test]
    fn padding_hides_length_to_the_block() {
        let a = pad(b"hi", 256);
        let b = pad(&[1u8; 200], 256);
        assert_eq!(a.len(), 256);
        assert_eq!(b.len(), 256);
        assert_eq!(unpad(&a).unwrap(), b"hi");
        assert_eq!(pad(&[0u8; 252], 256).len(), 256);
        assert_eq!(pad(&[0u8; 253], 256).len(), 512);
        assert!(unpad(&[0, 0, 0, 9, 1]).is_err());
    }

    #[test]
    fn hkdf_separates_purposes() {
        assert_ne!(
            *derive_key(b"k", b"s", b"ops"),
            *derive_key(b"k", b"s", b"blobs")
        );
        assert_ne!(
            *derive_key(b"k", b"s1", b"ops"),
            *derive_key(b"k", b"s2", b"ops")
        );
    }
}
