//! Pairing a new device (D-23, D-94).
//!
//! The existing device makes a **pairing code**: a 40-bit invitation id
//! (the server's name for the invitation) and an 80-bit pairing secret that
//! never reaches the server, written as 24 Base32 characters in six groups
//! of four. The QR code carries the same code and the server address.
//!
//! Every pairing message is XChaCha20-Poly1305 under
//! `HKDF-SHA256(ikm = secret, salt = invitation id, info =
//! "housewright/v1/pairing")`, with `aad = transcript(label, invitation id)`
//! where the label names the part (offer, request, reveal), so the
//! server relays them without being able to read or change them.
//!
//! Defence in depth against a leaked code (a photographed QR code, say):
//! both screens show a six-digit **safety number** that the person compares.
//! It hashes both devices' public keys and a 32-byte random value `r` from
//! the inviting device, which commits to `SHA-256(r)` in the offer and
//! reveals `r` only after the new device's keys have arrived. Someone in
//! the middle must therefore fix their keys before learning `r`, and
//! cannot search for keys that make the two numbers match (the ZRTP
//! commitment trick). The chance of a match by luck is one in a million.

use zeroize::Zeroizing;

use crate::encoding::{base32, group, hex, unbase32};
use crate::primitives::{derive_key, open, random, seal, sha256, transcript};
use crate::{CryptoError, DevicePublic, Result};

#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode {
    invitation: [u8; 5],
    secret: Zeroizing<[u8; 10]>,
}

impl std::fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PairingCode({}, ..)", self.invitation_id())
    }
}

/// Which pairing message.
#[derive(Clone, Copy, Debug)]
pub enum Part {
    Offer,
    Request,
    Reveal,
}

impl Part {
    fn label(self) -> &'static str {
        match self {
            Part::Offer => "housewright/v1/pair-offer",
            Part::Request => "housewright/v1/pair-request",
            Part::Reveal => "housewright/v1/pair-reveal",
        }
    }
}

impl PairingCode {
    pub fn generate() -> Result<Self> {
        Ok(Self {
            invitation: random()?,
            secret: Zeroizing::new(random()?),
        })
    }

    /// The server's name for the invitation (10 hex characters).
    pub fn invitation_id(&self) -> String {
        hex(&self.invitation)
    }

    /// `ABCD-EFGH-IJKL-MNOP-QRST-UVWX`.
    pub fn display(&self) -> Zeroizing<String> {
        let mut all = self.invitation.to_vec();
        all.extend_from_slice(self.secret.as_ref());
        let s = Zeroizing::new(base32(&all));
        Zeroizing::new(group(&s, 4))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let b = Zeroizing::new(unbase32(text, 15).map_err(|_| {
            CryptoError::Malformed("that code is not complete; it has 24 letters and digits".into())
        })?);
        let mut invitation = [0u8; 5];
        invitation.copy_from_slice(&b[..5]);
        let mut secret = Zeroizing::new([0u8; 10]);
        secret.copy_from_slice(&b[5..]);
        Ok(Self { invitation, secret })
    }

    fn key(&self) -> Zeroizing<[u8; 32]> {
        derive_key(
            self.secret.as_ref(),
            &self.invitation,
            b"housewright/v1/pairing",
        )
    }

    pub fn seal(&self, part: Part, plaintext: &[u8]) -> Result<Vec<u8>> {
        seal(
            &self.key(),
            plaintext,
            &transcript(part.label(), &[&self.invitation]),
        )
    }

    /// Fails (as [`CryptoError::Decrypt`]) when the code is wrong.
    pub fn open(&self, part: Part, data: &[u8]) -> Result<Vec<u8>> {
        open(
            &self.key(),
            data,
            &transcript(part.label(), &[&self.invitation]),
        )
    }
}

pub fn commitment(r: &[u8; 32]) -> [u8; 32] {
    sha256(&transcript("housewright/v1/pair-commit", &[r]))
}

/// Six digits, shown as `123 456`.
pub fn safety_number(
    invitation_id: &str,
    r: &[u8; 32],
    inviter: &DevicePublic,
    joiner: &DevicePublic,
) -> String {
    let h = sha256(&transcript(
        "housewright/v1/safety-number",
        &[
            invitation_id.as_bytes(),
            r,
            &inviter.sign,
            &inviter.dh,
            &joiner.sign,
            &joiner.dh,
        ],
    ));
    let n = u64::from_be_bytes(h[..8].try_into().expect("8 bytes")) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeviceSecret;

    #[test]
    fn code_round_trip_and_wrong_code() {
        let c = PairingCode::generate().unwrap();
        let shown = c.display();
        assert_eq!(shown.len(), 24 + 5);
        let typed = PairingCode::parse(&shown.to_lowercase()).unwrap();
        assert_eq!(typed, c);
        let ct = c.seal(Part::Offer, b"offer").unwrap();
        assert_eq!(typed.open(Part::Offer, &ct).unwrap(), b"offer");
        assert!(c.open(Part::Request, &ct).is_err(), "parts are separated");
        let wrong = PairingCode::generate().unwrap();
        assert_eq!(wrong.open(Part::Offer, &ct), Err(CryptoError::Decrypt));
        assert!(PairingCode::parse("ABCD-EFGH").is_err());
    }

    #[test]
    fn safety_number_depends_on_every_key() {
        let a = DeviceSecret::generate().unwrap().public();
        let b = DeviceSecret::generate().unwrap().public();
        let m = DeviceSecret::generate().unwrap().public();
        let r = [5u8; 32];
        let n = safety_number("inv", &r, &a, &b);
        assert_eq!(n.len(), 7);
        assert_eq!(n, safety_number("inv", &r, &a, &b));
        assert_ne!(n, safety_number("inv", &r, &a, &m));
        assert_ne!(n, safety_number("inv", &[6u8; 32], &a, &b));
        assert_ne!(commitment(&r), commitment(&[6u8; 32]));
    }
}
