//! Device identities and the household key chain.
//!
//! * A **device** has two key pairs, both made from 32-byte random seeds:
//!   Ed25519 for signing and X25519 (through HPKE's `DeriveKeyPair`) for
//!   receiving wrapped household keys. Its id is the first 16 bytes of
//!   `SHA-256(transcript("housewright/v1/device-id", ed25519 public key))`,
//!   so an id can never be paired with someone else's signing key.
//! * A **space** is the household's opaque id on a server: 16 random bytes,
//!   unrelated to the household's id inside the data (that one is a
//!   UUIDv7 and contains a timestamp).
//! * The **key chain** is every household key the household has had, one
//!   per epoch, oldest first. Removing a member starts a new epoch; the
//!   chain is what is wrapped to each device, so old data stays readable to
//!   everyone who remains.

use std::fmt;
use std::str::FromStr;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem, Serializable};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::encoding::{b64, hex, unb64_array, unhex_array};
use crate::primitives::{derive_key, random, random_secret, sha256, transcript};
use crate::{CryptoError, Result};

macro_rules! id_type {
    ($name:ident, $len:expr, $what:expr) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub [u8; $len]);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&hex(&self.0))
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), hex(&self.0))
            }
        }
        impl FromStr for $name {
            type Err = CryptoError;
            fn from_str(s: &str) -> Result<Self> {
                if s.len() != $len * 2 || s.bytes().any(|b| b.is_ascii_uppercase()) {
                    return Err(CryptoError::Malformed(format!("bad {}", $what)));
                }
                Ok(Self(unhex_array(s)?))
            }
        }
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(
                &self,
                s: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(
                d: D,
            ) -> std::result::Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

id_type!(DeviceId, 16, "device id");
id_type!(SpaceId, 16, "household id");

impl SpaceId {
    pub fn random() -> Result<Self> {
        Ok(Self(random()?))
    }
}

impl DeviceId {
    pub fn of_signing_key(pk: &[u8; 32]) -> Self {
        let h = sha256(&transcript("housewright/v1/device-id", &[pk]));
        let mut id = [0u8; 16];
        id.copy_from_slice(&h[..16]);
        DeviceId(id)
    }
}

/// Serde helper: 32 bytes as base64.
pub mod b64_32 {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &[u8; 32],
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&b64(v))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        unb64_array(&s).map_err(serde::de::Error::custom)
    }
}

/// Serde helper: 64 bytes as base64.
pub mod b64_64 {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &[u8; 64],
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&b64(v))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<[u8; 64], D::Error> {
        let s = String::deserialize(d)?;
        unb64_array(&s).map_err(serde::de::Error::custom)
    }
}

/// Serde helper: any bytes as base64.
pub mod b64_vec {
    use super::*;
    pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&b64(v))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        crate::encoding::unb64(&s).map_err(serde::de::Error::custom)
    }
}

/// What everyone may know about a device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePublic {
    pub id: DeviceId,
    /// Ed25519 verifying key.
    #[serde(with = "b64_32")]
    pub sign: [u8; 32],
    /// X25519 public key (HPKE recipient key).
    #[serde(with = "b64_32")]
    pub dh: [u8; 32],
}

impl DevicePublic {
    /// Check that the id matches the signing key and both keys are valid.
    pub fn check(&self) -> Result<()> {
        if DeviceId::of_signing_key(&self.sign) != self.id {
            return Err(CryptoError::Rejected(
                "device id does not match its key".into(),
            ));
        }
        VerifyingKey::from_bytes(&self.sign)
            .map_err(|_| CryptoError::Malformed("bad signing key".into()))?;
        <X25519HkdfSha256 as Kem>::PublicKey::from_bytes(&self.dh)
            .map_err(|_| CryptoError::Malformed("bad key-agreement key".into()))?;
        Ok(())
    }

    pub fn verify(&self, msg: &[u8], sig: &[u8; 64]) -> Result<()> {
        verify(&self.sign, msg, sig)
    }

    pub(crate) fn hpke_public(&self) -> Result<<X25519HkdfSha256 as Kem>::PublicKey> {
        <X25519HkdfSha256 as Kem>::PublicKey::from_bytes(&self.dh)
            .map_err(|_| CryptoError::Malformed("bad key-agreement key".into()))
    }
}

/// Verify an Ed25519 signature (strict: rejects small-order keys and
/// non-canonical signatures).
pub fn verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> Result<()> {
    let vk = VerifyingKey::from_bytes(pk).map_err(|_| CryptoError::Signature)?;
    vk.verify_strict(msg, &Signature::from_bytes(sig))
        .map_err(|_| CryptoError::Signature)
}

/// A device's secret seeds. Wiped from memory when dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DeviceSecret {
    sign_seed: [u8; 32],
    dh_seed: [u8; 32],
}

impl fmt::Debug for DeviceSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceSecret({})", self.id())
    }
}

impl DeviceSecret {
    pub fn generate() -> Result<Self> {
        let s = random_secret::<32>()?;
        let d = random_secret::<32>()?;
        Ok(Self::from_seeds(*s, *d))
    }

    pub fn from_seeds(sign_seed: [u8; 32], dh_seed: [u8; 32]) -> Self {
        Self { sign_seed, dh_seed }
    }

    /// Derive a device from one secret (the recovery key does this).
    pub fn derive(ikm: &[u8], label: &str) -> Self {
        let s = derive_key(
            ikm,
            b"housewright/v1/derived-device",
            format!("{label}/sign").as_bytes(),
        );
        let d = derive_key(
            ikm,
            b"housewright/v1/derived-device",
            format!("{label}/dh").as_bytes(),
        );
        Self::from_seeds(*s, *d)
    }

    /// 64 bytes: signing seed then key-agreement seed.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut v = Zeroizing::new(Vec::with_capacity(64));
        v.extend_from_slice(&self.sign_seed);
        v.extend_from_slice(&self.dh_seed);
        v
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        if b.len() != 64 {
            return Err(CryptoError::Malformed(
                "device secret is not 64 bytes".into(),
            ));
        }
        let mut s = [0u8; 32];
        let mut d = [0u8; 32];
        s.copy_from_slice(&b[..32]);
        d.copy_from_slice(&b[32..]);
        let out = Self::from_seeds(s, d);
        s.zeroize();
        d.zeroize();
        Ok(out)
    }

    fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.sign_seed)
    }

    pub(crate) fn hpke_keypair(
        &self,
    ) -> (
        <X25519HkdfSha256 as Kem>::PrivateKey,
        <X25519HkdfSha256 as Kem>::PublicKey,
    ) {
        X25519HkdfSha256::derive_keypair(&self.dh_seed)
    }

    pub fn public(&self) -> DevicePublic {
        let sign = self.signing_key().verifying_key().to_bytes();
        let (_, pk) = self.hpke_keypair();
        let mut dh = [0u8; 32];
        dh.copy_from_slice(&pk.to_bytes());
        DevicePublic {
            id: DeviceId::of_signing_key(&sign),
            sign,
            dh,
        }
    }

    pub fn id(&self) -> DeviceId {
        DeviceId::of_signing_key(&self.signing_key().verifying_key().to_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.signing_key().sign(msg).to_bytes()
    }
}

/// One epoch's household key.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct HouseholdKey([u8; 32]);

/// Every household key so far; index = epoch.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyChain {
    keys: Vec<HouseholdKey>,
}

impl fmt::Debug for KeyChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyChain(epochs 0..={})", self.current_epoch())
    }
}

/// What a derived key is for. Each purpose gets its own HKDF info string,
/// so a key for one purpose is useless for any other.
#[derive(Clone, Copy, Debug)]
pub enum Purpose {
    /// Operation batches.
    Ops,
    /// File contents.
    Blobs,
    /// Device names in the roster.
    Labels,
    /// The keyed hash that turns a file's SHA-256 into its server name.
    /// Always taken from epoch 0, so a file keeps its name across epochs.
    BlobNames,
}

impl Purpose {
    fn info(self) -> &'static str {
        match self {
            Purpose::Ops => "housewright/v1/key/ops",
            Purpose::Blobs => "housewright/v1/key/blobs",
            Purpose::Labels => "housewright/v1/key/labels",
            Purpose::BlobNames => "housewright/v1/key/blob-names",
        }
    }
}

impl KeyChain {
    /// A new household: one random key, epoch 0.
    pub fn generate() -> Result<Self> {
        Ok(Self {
            keys: vec![HouseholdKey(*random_secret::<32>()?)],
        })
    }

    /// The chain with one more, fresh, key (a new epoch).
    pub fn rotated(&self) -> Result<Self> {
        let mut keys = self.keys.clone();
        keys.push(HouseholdKey(*random_secret::<32>()?));
        Ok(Self { keys })
    }

    pub fn current_epoch(&self) -> u32 {
        (self.keys.len() - 1) as u32
    }

    /// A purpose key for `epoch`: HKDF-SHA256(ikm = household key of that
    /// epoch, salt = space id, info = purpose label || u32be(epoch)).
    pub fn key(&self, space: SpaceId, epoch: u32, purpose: Purpose) -> Result<Zeroizing<[u8; 32]>> {
        let k = self
            .keys
            .get(epoch as usize)
            .ok_or_else(|| CryptoError::Rejected(format!("no key for epoch {epoch}")))?;
        let mut info = purpose.info().as_bytes().to_vec();
        info.extend_from_slice(&epoch.to_be_bytes());
        Ok(derive_key(&k.0, &space.0, &info))
    }

    /// `u32be(epochs) || key_0 || key_1 || ...`
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut v = Zeroizing::new(Vec::with_capacity(4 + 32 * self.keys.len()));
        v.extend_from_slice(&(self.keys.len() as u32).to_be_bytes());
        for k in &self.keys {
            v.extend_from_slice(&k.0);
        }
        v
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        let bad = || CryptoError::Malformed("bad key chain".into());
        let n =
            u32::from_be_bytes(b.get(..4).ok_or_else(bad)?.try_into().map_err(|_| bad())?) as usize;
        if n == 0 || n.checked_mul(32).and_then(|x| x.checked_add(4)) != Some(b.len()) {
            return Err(bad());
        }
        let keys = b[4..]
            .as_chunks::<32>()
            .0
            .iter()
            .map(|c| HouseholdKey(*c))
            .collect();
        Ok(Self { keys })
    }

    /// True if `other` is this chain plus zero or more later epochs.
    pub fn is_prefix_of(&self, other: &KeyChain) -> bool {
        self.keys.len() <= other.keys.len()
            && self.keys.iter().zip(&other.keys).all(|(a, b)| a == b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_round_trip() {
        let d = DeviceSecret::generate().unwrap();
        let p = d.public();
        p.check().unwrap();
        let d2 = DeviceSecret::from_bytes(&d.to_bytes()).unwrap();
        assert_eq!(d2.public(), p);
        let sig = d.sign(b"hello");
        p.verify(b"hello", &sig).unwrap();
        assert!(p.verify(b"hellO", &sig).is_err());
        let mut forged = p.clone();
        forged.sign = DeviceSecret::generate().unwrap().public().sign;
        assert!(forged.check().is_err(), "id is bound to the signing key");
    }

    #[test]
    fn ids_parse_strictly() {
        let id = DeviceSecret::generate().unwrap().id();
        assert_eq!(id.to_string().parse::<DeviceId>().unwrap(), id);
        assert!(id.to_string().to_uppercase().parse::<DeviceId>().is_err());
        assert!("abc".parse::<DeviceId>().is_err());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<DeviceId>(&json).unwrap(), id);
    }

    #[test]
    fn chain_keys_differ_by_epoch_purpose_and_space() {
        let c = KeyChain::generate().unwrap().rotated().unwrap();
        let s = SpaceId::random().unwrap();
        let s2 = SpaceId::random().unwrap();
        let a = c.key(s, 0, Purpose::Ops).unwrap();
        assert_ne!(*a, *c.key(s, 1, Purpose::Ops).unwrap());
        assert_ne!(*a, *c.key(s, 0, Purpose::Blobs).unwrap());
        assert_ne!(*a, *c.key(s2, 0, Purpose::Ops).unwrap());
        assert!(c.key(s, 2, Purpose::Ops).is_err());
        let c2 = KeyChain::from_bytes(&c.to_bytes()).unwrap();
        assert_eq!(c2, c);
        assert_eq!(c2.current_epoch(), 1);
        let older = KeyChain::from_bytes(&c.to_bytes()[..0]);
        assert!(older.is_err());
    }
}
