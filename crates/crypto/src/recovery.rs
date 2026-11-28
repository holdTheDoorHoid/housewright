//! The printable recovery key (D-15, D-91).
//!
//! 160 random bits, written as 32 Base32 characters plus 4 checksum
//! characters, in nine groups of four: `ABCD-EFGH-...-WXYZ`. The checksum
//! (20 bits of SHA-256) catches typing mistakes; it adds no secrecy.
//!
//! The recovery key is never stored on a device after it has been shown.
//! From it, anyone holding the paper derives:
//!
//! * a **recovery identity** (an Ed25519 and an X25519 key pair, through
//!   [`DeviceSecret::derive`]) that sits in the roster with owner powers,
//!   so the household key chain is wrapped to it like to any device, and it
//!   can add a fresh device after every device is lost;
//! * an **anchor key**: XChaCha20-Poly1305 over the hash of the genesis
//!   roster, stored on the server. Only someone with the paper could have
//!   made it, so a restoring device cannot be fed a fake household history.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::encoding::{base32, group, unbase32};
use crate::primitives::{derive_key, open, random_secret, seal, sha256, transcript};
use crate::{CryptoError, DeviceSecret, Result, SpaceId};

const KEY_LEN: usize = 20;
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct RecoveryKey([u8; KEY_LEN]);

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey(..)")
    }
}

fn checksum(key: &[u8; KEY_LEN]) -> String {
    let h = sha256(&transcript("housewright/v1/recovery-checksum", &[key]));
    let bits = ((h[0] as u32) << 16) | ((h[1] as u32) << 8) | h[2] as u32;
    (0..4)
        .map(|i| ALPHABET[((bits >> (19 - 5 * i)) & 31) as usize] as char)
        .collect()
}

/// What the anchor proves to a restoring device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// Hash of the genesis roster (hex).
    pub genesis: String,
}

impl RecoveryKey {
    pub fn generate() -> Result<Self> {
        Ok(Self(*random_secret::<KEY_LEN>()?))
    }

    /// `ABCD-EFGH-...`, nine groups of four. Shown once, printed once.
    pub fn display(&self) -> Zeroizing<String> {
        let mut s = base32(&self.0);
        s.push_str(&checksum(&self.0));
        let out = Zeroizing::new(group(&s, 4));
        s.zeroize();
        out
    }

    /// Read a typed key. Case, spaces and dashes do not matter.
    pub fn parse(text: &str) -> Result<Self> {
        let clean: Zeroizing<String> = Zeroizing::new(
            text.chars()
                .filter(|c| !c.is_whitespace() && *c != '-')
                .collect(),
        );
        if clean.len() != 36 {
            return Err(CryptoError::Malformed(
                "a recovery key has 36 letters and digits".into(),
            ));
        }
        let (body, sum) = clean.split_at(32);
        let bytes = Zeroizing::new(unbase32(body, KEY_LEN)?);
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(&bytes);
        let key = Self(k);
        k.zeroize();
        let typed_sum = sum
            .to_ascii_uppercase()
            .replace('0', "O")
            .replace('1', "I")
            .replace('8', "B");
        if typed_sum != checksum(&key.0) {
            return Err(CryptoError::Malformed(
                "that recovery key has a typing mistake".into(),
            ));
        }
        Ok(key)
    }

    pub fn identity(&self) -> DeviceSecret {
        DeviceSecret::derive(&self.0, "housewright/v1/recovery")
    }

    fn anchor_key(&self) -> Zeroizing<[u8; 32]> {
        derive_key(&self.0, b"housewright/v1/recovery", b"anchor")
    }

    pub fn seal_anchor(&self, space: SpaceId, anchor: &Anchor) -> Result<Vec<u8>> {
        let pt = serde_json::to_vec(anchor).map_err(|e| CryptoError::Malformed(e.to_string()))?;
        seal(
            &self.anchor_key(),
            &pt,
            &transcript("housewright/v1/anchor", &[&space.0]),
        )
    }

    pub fn open_anchor(&self, space: SpaceId, data: &[u8]) -> Result<Anchor> {
        let pt = open(
            &self.anchor_key(),
            data,
            &transcript("housewright/v1/anchor", &[&space.0]),
        )?;
        serde_json::from_slice(&pt).map_err(|e| CryptoError::Malformed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_parse_and_typos() {
        let k = RecoveryKey::generate().unwrap();
        let shown = k.display();
        assert_eq!(shown.len(), 36 + 8);
        assert_eq!(shown.matches('-').count(), 8);
        assert_eq!(RecoveryKey::parse(&shown).unwrap(), k);
        assert_eq!(
            RecoveryKey::parse(&shown.to_lowercase().replace('-', " ")).unwrap(),
            k
        );
        // One wrong character is caught by the checksum (or the alphabet).
        let mut chars: Vec<char> = shown.chars().collect();
        chars[5] = if chars[5] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        assert!(RecoveryKey::parse(&typo).is_err());
        assert!(RecoveryKey::parse("ABCD").is_err());
    }

    #[test]
    fn identity_is_stable_and_anchor_binds_space() {
        let k = RecoveryKey::generate().unwrap();
        let again = RecoveryKey::parse(&k.display()).unwrap();
        assert_eq!(k.identity().public(), again.identity().public());
        let s = SpaceId::random().unwrap();
        let a = Anchor {
            genesis: "ab".repeat(32),
        };
        let sealed = k.seal_anchor(s, &a).unwrap();
        assert_eq!(again.open_anchor(s, &sealed).unwrap(), a);
        assert!(k.open_anchor(SpaceId::random().unwrap(), &sealed).is_err());
        assert!(
            RecoveryKey::generate()
                .unwrap()
                .open_anchor(s, &sealed)
                .is_err()
        );
    }
}
