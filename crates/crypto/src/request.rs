//! Signed requests: how a device proves who it is to the server.
//!
//! No accounts and no session tokens: every request carries
//!
//! * `hw-device`: the device id (hex),
//! * `hw-time`: the device's clock in milliseconds,
//! * `hw-nonce`: 16 random bytes (hex),
//! * `hw-sig`: `Ed25519(device, transcript("housewright/v1/request",
//!   METHOD, path-and-query, u64be(time), nonce, SHA-256(body)))` (base64).
//!
//! The server refuses a request whose time is more than five minutes from
//! its own clock, and remembers every (device, nonce) it accepted for that
//! window, so a captured request cannot be replayed. Chosen over a
//! challenge-response session (D-90): nothing secret is ever issued by the
//! server, nothing can leak from a log or a proxy, a server restart loses
//! nothing, and every request is bound to its exact body.

use crate::encoding::{b64, hex, unb64_array, unhex_array};
use crate::primitives::{random, sha256, transcript};
use crate::{CryptoError, DeviceId, DevicePublic, DeviceSecret, Result};

pub const H_DEVICE: &str = "hw-device";
pub const H_TIME: &str = "hw-time";
pub const H_NONCE: &str = "hw-nonce";
pub const H_SIG: &str = "hw-sig";
/// Response header: the server's clock in milliseconds.
pub const H_SERVER_TIME: &str = "hw-server-time";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestAuth {
    pub device: DeviceId,
    pub time_ms: u64,
    pub nonce: [u8; 16],
    pub sig: [u8; 64],
}

fn input(method: &str, path: &str, time_ms: u64, nonce: &[u8; 16], body: &[u8]) -> Vec<u8> {
    transcript(
        "housewright/v1/request",
        &[
            method.to_ascii_uppercase().as_bytes(),
            path.as_bytes(),
            &time_ms.to_be_bytes(),
            nonce,
            &sha256(body),
        ],
    )
}

pub fn sign_request(
    device: &DeviceSecret,
    method: &str,
    path_and_query: &str,
    time_ms: u64,
    body: &[u8],
) -> Result<RequestAuth> {
    let nonce = random::<16>()?;
    Ok(RequestAuth {
        device: device.id(),
        time_ms,
        nonce,
        sig: device.sign(&input(method, path_and_query, time_ms, &nonce, body)),
    })
}

pub fn verify_request(
    device: &DevicePublic,
    auth: &RequestAuth,
    method: &str,
    path_and_query: &str,
    body: &[u8],
) -> Result<()> {
    if device.id != auth.device {
        return Err(CryptoError::Rejected("request device mismatch".into()));
    }
    device.verify(
        &input(method, path_and_query, auth.time_ms, &auth.nonce, body),
        &auth.sig,
    )
}

impl RequestAuth {
    pub fn headers(&self) -> [(&'static str, String); 4] {
        [
            (H_DEVICE, self.device.to_string()),
            (H_TIME, self.time_ms.to_string()),
            (H_NONCE, hex(&self.nonce)),
            (H_SIG, b64(&self.sig)),
        ]
    }

    /// Read the four headers (`get` looks one up by name).
    pub fn from_headers<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Result<Self> {
        let need = |n: &str| get(n).ok_or_else(|| CryptoError::Malformed(format!("missing {n}")));
        Ok(Self {
            device: need(H_DEVICE)?.parse()?,
            time_ms: need(H_TIME)?
                .parse()
                .map_err(|_| CryptoError::Malformed("bad hw-time".into()))?,
            nonce: unhex_array(need(H_NONCE)?)?,
            sig: unb64_array(need(H_SIG)?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_bind_everything() {
        let d = DeviceSecret::generate().unwrap();
        let p = d.public();
        let a = sign_request(&d, "post", "/v1/s/ab/ops", 1234, b"{}").unwrap();
        verify_request(&p, &a, "POST", "/v1/s/ab/ops", b"{}").unwrap();
        assert!(verify_request(&p, &a, "PUT", "/v1/s/ab/ops", b"{}").is_err());
        assert!(verify_request(&p, &a, "POST", "/v1/s/ac/ops", b"{}").is_err());
        assert!(verify_request(&p, &a, "POST", "/v1/s/ab/ops", b"{ }").is_err());
        let mut t = a.clone();
        t.time_ms += 1;
        assert!(verify_request(&p, &t, "POST", "/v1/s/ab/ops", b"{}").is_err());
        let headers = a.headers();
        let back = RequestAuth::from_headers(|n| {
            headers
                .iter()
                .find(|(k, _)| *k == n)
                .map(|(_, v)| v.as_str())
        })
        .unwrap();
        assert_eq!(back, a);
    }
}
