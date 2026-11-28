//! Encrypted files (photos, documents) on the server.
//!
//! * Name on the server: `hex(HMAC-SHA256(blob-name key, sha256 hex)[..16])`.
//!   The key comes from epoch 0, so a file keeps its name for ever and is
//!   uploaded once; the server cannot test "is this known file here?"
//!   without the key.
//! * Content: `"HWB1" || u32be(epoch) || nonce(24) || XChaCha20-Poly1305(
//!   blob key of the epoch, nonce, pad(bytes, 4096), aad)` with
//!   `aad = transcript("housewright/v1/blob", space, name, u32be(epoch))`.
//! * After decrypting, the receiver checks the SHA-256 of the plaintext
//!   against the hash in the (signed) attachment operation, so a file
//!   cannot be swapped for another one even by a household member.

use crate::encoding::hex;
use crate::keys::Purpose;
use crate::primitives::{
    NONCE_LEN, hmac_sha256, open_with_nonce, pad, random, seal_with_nonce, sha256, transcript,
    unpad,
};
use crate::{CryptoError, KeyChain, Result, SpaceId};

pub const BLOB_PAD: usize = 4096;
const MAGIC: &[u8; 4] = b"HWB1";

pub fn blob_name(chain: &KeyChain, space: SpaceId, sha256_hex: &str) -> Result<String> {
    let k = chain.key(space, 0, Purpose::BlobNames)?;
    Ok(hex(&hmac_sha256(k.as_ref(), sha256_hex.as_bytes())[..16]))
}

fn aad(space: SpaceId, name: &str, epoch: u32) -> Vec<u8> {
    transcript(
        "housewright/v1/blob",
        &[&space.0, name.as_bytes(), &epoch.to_be_bytes()],
    )
}

pub fn seal_blob(chain: &KeyChain, space: SpaceId, name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let epoch = chain.current_epoch();
    let key = chain.key(space, epoch, Purpose::Blobs)?;
    let nonce = random::<NONCE_LEN>()?;
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&nonce);
    out.extend(seal_with_nonce(
        &key,
        &nonce,
        &pad(bytes, BLOB_PAD),
        &aad(space, name, epoch),
    )?);
    Ok(out)
}

/// Decrypt and check the content hash.
pub fn open_blob(
    chain: &KeyChain,
    space: SpaceId,
    name: &str,
    data: &[u8],
    sha256_hex: &str,
) -> Result<Vec<u8>> {
    if data.len() < 8 + NONCE_LEN || &data[..4] != MAGIC {
        return Err(CryptoError::Malformed("not an encrypted file".into()));
    }
    let epoch = u32::from_be_bytes(data[4..8].try_into().expect("4 bytes"));
    let nonce: [u8; NONCE_LEN] = data[8..8 + NONCE_LEN].try_into().expect("24 bytes");
    let key = chain.key(space, epoch, Purpose::Blobs)?;
    let padded = open_with_nonce(
        &key,
        &nonce,
        &data[8 + NONCE_LEN..],
        &aad(space, name, epoch),
    )?;
    let bytes = unpad(&padded)?;
    if hex(&sha256(&bytes)) != sha256_hex {
        return Err(CryptoError::Rejected("file does not match its hash".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_round_trip_and_bind_their_name() {
        let c = KeyChain::generate().unwrap();
        let s = SpaceId::random().unwrap();
        let bytes = b"a photo of the water heater".to_vec();
        let h = hex(&sha256(&bytes));
        let name = blob_name(&c, s, &h).unwrap();
        let sealed = seal_blob(&c, s, &name, &bytes).unwrap();
        assert_eq!(sealed.len(), 8 + 24 + BLOB_PAD + 16);
        assert_eq!(open_blob(&c, s, &name, &sealed, &h).unwrap(), bytes);
        let other = blob_name(&c, s, &"0".repeat(64)).unwrap();
        assert!(
            open_blob(&c, s, &other, &sealed, &h).is_err(),
            "moved to another name"
        );
        assert!(
            open_blob(&c, s, &name, &sealed, &"1".repeat(64)).is_err(),
            "wrong hash"
        );
        let mut t = sealed.clone();
        t[40] ^= 1;
        assert!(open_blob(&c, s, &name, &t, &h).is_err());
        // Names survive rotation.
        let c2 = c.rotated().unwrap();
        assert_eq!(blob_name(&c2, s, &h).unwrap(), name);
        assert_eq!(open_blob(&c2, s, &name, &sealed, &h).unwrap(), bytes);
    }
}
