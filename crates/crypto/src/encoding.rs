//! Text encodings for the wire (JSON) and for people (Base32 codes).
//! None of this is cryptography; it only moves bytes into text and back.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::{CryptoError, Result};

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|_| CryptoError::Malformed("bad base64".into()))
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(CryptoError::Malformed("bad hex".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            s.get(i..i + 2)
                .and_then(|p| u8::from_str_radix(p, 16).ok())
                .ok_or_else(|| CryptoError::Malformed("bad hex".into()))
        })
        .collect()
}

pub fn unhex_array<const N: usize>(s: &str) -> Result<[u8; N]> {
    unhex(s)?
        .try_into()
        .map_err(|_| CryptoError::Malformed(format!("expected {N} bytes")))
}

pub fn unb64_array<const N: usize>(s: &str) -> Result<[u8; N]> {
    unb64(s)?
        .try_into()
        .map_err(|_| CryptoError::Malformed(format!("expected {N} bytes")))
}

/// RFC 4648 Base32 alphabet. It has no 0, 1, 8 or 9, so a person cannot
/// confuse O with 0 or I with 1 when typing a code back in.
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 without padding. `bytes.len() * 8` must be a multiple of 5.
pub fn base32(bytes: &[u8]) -> String {
    debug_assert!((bytes.len() * 8).is_multiple_of(5));
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &b in bytes {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Parse Base32 typed by a person: case, spaces and dashes are ignored, and
/// the digits 0, 1 and 8 are read as O, I and B.
pub fn unbase32(s: &str, len: usize) -> Result<Vec<u8>> {
    let mut acc: u64 = 0;
    let mut bits = 0;
    let mut out = Vec::with_capacity(len);
    for c in s.chars() {
        if c.is_whitespace() || c == '-' {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            '0' => 'O',
            '1' => 'I',
            '8' => 'B',
            c => c,
        };
        let v = ALPHABET
            .iter()
            .position(|&a| a as char == c)
            .ok_or_else(|| CryptoError::Malformed(format!("'{c}' is not used in codes")))?;
        acc = (acc << 5) | v as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    if out.len() != len {
        return Err(CryptoError::Malformed(
            "the code is the wrong length".into(),
        ));
    }
    Ok(out)
}

/// Group a code for reading aloud or writing down: `ABCD-EFGH-...`.
pub fn group(s: &str, n: usize) -> String {
    s.as_bytes()
        .chunks(n)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_round_trip_and_lenient_reading() {
        let bytes = [0u8, 1, 2, 250, 251, 252, 253, 254, 255, 9];
        let s = base32(&bytes);
        assert_eq!(s.len(), 16);
        assert_eq!(unbase32(&s, 10).unwrap(), bytes);
        let typed = group(&s.to_lowercase(), 4).replace('O', "0");
        assert_eq!(unbase32(&typed, 10).unwrap(), bytes);
        assert!(unbase32("ABC", 10).is_err());
        assert!(unbase32("!!!!", 2).is_err());
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(unhex(&hex(&[1, 171, 255])).unwrap(), vec![1, 171, 255]);
        assert!(unhex("abc").is_err());
        assert!(unhex("zz").is_err());
    }
}
