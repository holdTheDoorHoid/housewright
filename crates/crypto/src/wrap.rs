//! Key envelopes: the household key chain sealed to one device.
//!
//! HPKE (RFC 9180), base mode, suite DHKEM(X25519, HKDF-SHA256) /
//! HKDF-SHA256 / ChaCha20-Poly1305, single-shot:
//!
//! * `info = transcript("housewright/v1/wrap-info", space, recipient id,
//!   recipient X25519 key, u32be(epoch))`
//! * `aad  = transcript("housewright/v1/wrap-aad", sender id)`
//! * plaintext = the key chain bytes ([`KeyChain::to_bytes`]).
//!
//! HPKE base mode does not say who sealed the envelope, so the sender
//! signs it: `Ed25519(sender, transcript("housewright/v1/wrap-sig", space,
//! recipient id, u32be(epoch), sender id, enc, ciphertext))`. Receivers
//! check the signer is an owner (or the recovery identity) in the roster
//! before trusting the keys.

use hpke::aead::ChaCha20Poly1305;
use hpke::kdf::HkdfSha256;
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem, OpModeR, OpModeS, Serializable};
use serde::{Deserialize, Serialize};

use crate::keys::{b64_64, b64_vec};
use crate::primitives::transcript;
use crate::{CryptoError, DeviceId, DevicePublic, DeviceSecret, KeyChain, Result, SpaceId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEnvelope {
    pub space: SpaceId,
    pub recipient: DeviceId,
    pub epoch: u32,
    pub sender: DeviceId,
    #[serde(with = "b64_vec")]
    pub enc: Vec<u8>,
    #[serde(with = "b64_vec")]
    pub ct: Vec<u8>,
    #[serde(with = "b64_64")]
    pub sig: [u8; 64],
}

fn info(space: SpaceId, recipient: &DevicePublic, epoch: u32) -> Vec<u8> {
    transcript(
        "housewright/v1/wrap-info",
        &[
            &space.0,
            &recipient.id.0,
            &recipient.dh,
            &epoch.to_be_bytes(),
        ],
    )
}

fn aad(sender: DeviceId) -> Vec<u8> {
    transcript("housewright/v1/wrap-aad", &[&sender.0])
}

fn sig_input(e: &KeyEnvelope) -> Vec<u8> {
    transcript(
        "housewright/v1/wrap-sig",
        &[
            &e.space.0,
            &e.recipient.0,
            &e.epoch.to_be_bytes(),
            &e.sender.0,
            &e.enc,
            &e.ct,
        ],
    )
}

/// Seal `chain` (at its current epoch) to `recipient`, signed by `sender`.
pub fn wrap(
    sender: &DeviceSecret,
    space: SpaceId,
    recipient: &DevicePublic,
    chain: &KeyChain,
) -> Result<KeyEnvelope> {
    recipient.check()?;
    let epoch = chain.current_epoch();
    let pk = recipient.hpke_public()?;
    let sender_id = sender.id();
    let (enc, ct) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
        &OpModeS::Base,
        &pk,
        &info(space, recipient, epoch),
        &chain.to_bytes(),
        &aad(sender_id),
    )
    .map_err(|_| CryptoError::Malformed("could not seal the key envelope".into()))?;
    let mut e = KeyEnvelope {
        space,
        recipient: recipient.id,
        epoch,
        sender: sender_id,
        enc: enc.to_bytes().to_vec(),
        ct,
        sig: [0; 64],
    };
    e.sig = sender.sign(&sig_input(&e));
    Ok(e)
}

/// Check the sender's signature (with their key from the roster) and open.
pub fn unwrap(me: &DeviceSecret, env: &KeyEnvelope, sender: &DevicePublic) -> Result<KeyChain> {
    if sender.id != env.sender {
        return Err(CryptoError::Rejected("envelope sender mismatch".into()));
    }
    sender.verify(&sig_input(env), &env.sig)?;
    let mine = me.public();
    if env.recipient != mine.id {
        return Err(CryptoError::Rejected(
            "envelope is for another device".into(),
        ));
    }
    let (sk, _) = me.hpke_keypair();
    let enc = <X25519HkdfSha256 as Kem>::EncappedKey::from_bytes(&env.enc)
        .map_err(|_| CryptoError::Malformed("bad envelope".into()))?;
    let pt = hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
        &OpModeR::Base,
        &sk,
        &enc,
        &info(env.space, &mine, env.epoch),
        &env.ct,
        &aad(env.sender),
    )
    .map_err(|_| CryptoError::Decrypt)?;
    let chain = KeyChain::from_bytes(&zeroize::Zeroizing::new(pt))?;
    if chain.current_epoch() != env.epoch {
        return Err(CryptoError::Rejected("envelope epoch mismatch".into()));
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_unwrap_and_reject_tampering() {
        let owner = DeviceSecret::generate().unwrap();
        let phone = DeviceSecret::generate().unwrap();
        let other = DeviceSecret::generate().unwrap();
        let space = SpaceId::random().unwrap();
        let chain = KeyChain::generate().unwrap().rotated().unwrap();
        let env = wrap(&owner, space, &phone.public(), &chain).unwrap();
        assert_eq!(unwrap(&phone, &env, &owner.public()).unwrap(), chain);
        // Wrong recipient.
        assert!(unwrap(&other, &env, &owner.public()).is_err());
        // Claimed sender is not the signer.
        assert!(unwrap(&phone, &env, &other.public()).is_err());
        // A changed epoch breaks the signature.
        let mut e2 = env.clone();
        e2.epoch = 0;
        assert_eq!(
            unwrap(&phone, &e2, &owner.public()),
            Err(CryptoError::Signature)
        );
        // Moved to another space: signature, and the HPKE info, differ.
        let mut e3 = env.clone();
        e3.space = SpaceId::random().unwrap();
        assert!(unwrap(&phone, &e3, &owner.public()).is_err());
        // Re-signed by an attacker with a changed ciphertext.
        let mut e4 = env.clone();
        e4.ct[0] ^= 1;
        e4.sender = other.id();
        e4.sig = other.sign(&sig_input(&e4));
        assert_eq!(
            unwrap(&phone, &e4, &other.public()),
            Err(CryptoError::Decrypt)
        );
    }
}
