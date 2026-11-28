//! How a version of the library travels: one gzip-compressed JSON file
//! holding every TOML file's text (a *bundle*), and, for updates, a small
//! *release manifest* (`latest.json`) naming the newest bundle with its size
//! and SHA-256.
//!
//! The same bundle format is compiled into the app (built from the vendored
//! snapshot in `library/` by `crates/api/build.rs`) and downloaded by
//! "Get appliance library updates". Everything here is pure: bytes in,
//! values out, with hard size limits so a hostile file cannot exhaust
//! memory.

use std::collections::BTreeMap;
use std::io::Read;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const BUNDLE_FORMAT: &str = "housewright-library-bundle";
pub const MANIFEST_FORMAT: &str = "housewright-library-release";

/// A downloaded bundle may be at most this big, compressed.
pub const MAX_COMPRESSED: usize = 2 * 1024 * 1024;
/// ... and this big once decompressed.
pub const MAX_DECOMPRESSED: usize = 8 * 1024 * 1024;
/// At most this many files, each at most this big.
pub const MAX_FILES: usize = 5000;
pub const MAX_FILE_BYTES: usize = 64 * 1024;
/// A release manifest is tiny.
pub const MAX_MANIFEST: usize = 4096;

/// One version of the library: file path -> TOML text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub bundle: u32,
    pub files: BTreeMap<String, String>,
}

/// Paths a bundle may hold: the snapshot record, the two reference files,
/// and categories, brands and families named by valid ids.
pub fn path_allowed(path: &str) -> bool {
    fn stem(s: &str) -> Option<&str> {
        s.strip_suffix(".toml")
            .filter(|x| super::check::valid_id(x))
    }
    match path.split('/').collect::<Vec<_>>().as_slice() {
        ["SNAPSHOT.toml"] => true,
        ["reference", "templates.toml" | "domains.toml"] => true,
        ["categories", f] | ["brands", f] => stem(f).is_some(),
        ["families", b, f] => super::check::valid_id(b) && stem(f).is_some(),
        _ => false,
    }
}

impl Bundle {
    pub fn new(files: BTreeMap<String, String>) -> Self {
        Self {
            format: BUNDLE_FORMAT.into(),
            bundle: 1,
            files,
        }
    }

    /// Gzip-compressed JSON.
    pub fn to_gz(&self) -> Vec<u8> {
        use std::io::Write;
        let json = serde_json::to_vec(self).expect("a bundle serialises");
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&json).expect("writing to memory");
        enc.finish().expect("writing to memory")
    }

    /// Read a gzip-compressed bundle, refusing anything over the limits or
    /// any path a bundle may not hold.
    pub fn from_gz(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_COMPRESSED {
            return Err(format!(
                "the library file is {} bytes; at most {MAX_COMPRESSED} are accepted",
                bytes.len()
            ));
        }
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_DECOMPRESSED as u64 + 1)
            .read_to_end(&mut json)
            .map_err(|e| format!("the library file is not gzip data ({e})"))?;
        if json.len() > MAX_DECOMPRESSED {
            return Err("the library file unpacks to more than the limit".into());
        }
        Self::from_json(&json)
    }

    /// The uncompressed JSON (kept on the device after an update).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a bundle serialises")
    }

    /// Read uncompressed bundle JSON, with the same limits and path rules.
    pub fn from_json(json: &[u8]) -> Result<Self, String> {
        if json.len() > MAX_DECOMPRESSED {
            return Err("the library file is larger than the limit".into());
        }
        let b: Bundle = serde_json::from_slice(json)
            .map_err(|e| format!("the library file is not a library bundle ({e})"))?;
        if b.format != BUNDLE_FORMAT || b.bundle != 1 {
            return Err(format!(
                "the library file is format '{}' {}, which this app does not read",
                b.format, b.bundle
            ));
        }
        if b.files.len() > MAX_FILES {
            return Err("the library file holds too many files".into());
        }
        for (path, text) in &b.files {
            if !path_allowed(path) {
                return Err(format!(
                    "the library file holds an unexpected path '{path}'"
                ));
            }
            if text.len() > MAX_FILE_BYTES {
                return Err(format!("{path} is larger than {MAX_FILE_BYTES} bytes"));
            }
        }
        Ok(b)
    }
}

/// `latest.json`: what the newest published library is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    /// The library file format the bundle's files use (`format = 1`).
    pub library_format: u32,
    pub version: String,
    pub commit: String,
    pub published: String,
    /// File name of the bundle, beside `latest.json`.
    pub bundle: String,
    /// Lowercase hex SHA-256 of the bundle file as downloaded.
    pub sha256: String,
    pub size: u64,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_MANIFEST {
            return Err("the release note is too large".into());
        }
        let m: Manifest = serde_json::from_str(text)
            .map_err(|e| format!("the release note is not readable ({e})"))?;
        if m.format != MANIFEST_FORMAT {
            return Err("the release note is not a library release".into());
        }
        if m.library_format != 1 {
            return Err(format!(
                "the newest library uses format {}; this version of the app reads format 1. An app update will bring it.",
                m.library_format
            ));
        }
        parse_version(&m.version).ok_or_else(|| format!("'{}' is not a version", m.version))?;
        let name_ok = m
            .bundle
            .strip_prefix("library-")
            .and_then(|s| s.strip_suffix(".json.gz"))
            .is_some_and(|v| parse_version(v).is_some());
        if !name_ok {
            return Err(format!("'{}' is not a library file name", m.bundle));
        }
        if m.sha256.len() != 64
            || !m
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("the release note's checksum is malformed".into());
        }
        if m.size == 0 || m.size > MAX_COMPRESSED as u64 {
            return Err(format!(
                "the library file would be {} bytes; too large",
                m.size
            ));
        }
        if m.commit.is_empty() || !m.commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("the release note's commit is malformed".into());
        }
        Ok(m)
    }

    /// Does `bytes` match what this manifest promised?
    pub fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() as u64 != self.size {
            return Err(format!(
                "the library file is {} bytes, but the release says {}; not used",
                bytes.len(),
                self.size
            ));
        }
        if sha256_hex(bytes) != self.sha256 {
            return Err("the library file does not match its checksum; not used".into());
        }
        Ok(())
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `2026.11.28.1` -> [2026, 11, 28, 1]. At most six parts.
pub fn parse_version(v: &str) -> Option<Vec<u32>> {
    let parts: Option<Vec<u32>> = v.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty() && p.len() <= 6)
}

/// Is version `a` newer than `b`? (Unparseable versions are never newer.)
pub fn is_newer(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("2026.12.1", "2026.11.28"));
        assert!(is_newer("2026.11.28.1", "2026.11.28"));
        assert!(!is_newer("2026.11.28", "2026.11.28"));
        assert!(!is_newer("2026.2.1", "2026.11.28"));
        assert!(!is_newer("x", "2026.11.28"));
        assert!(parse_version("1.2.3.4.5.6.7").is_none());
    }

    #[test]
    fn paths() {
        assert!(path_allowed("brands/rheem.toml"));
        assert!(path_allowed("families/rheem/xg-gas.toml"));
        assert!(path_allowed("SNAPSHOT.toml"));
        assert!(!path_allowed("families/rheem/../x.toml"));
        assert!(!path_allowed("families/rheem/manual.pdf"));
        assert!(!path_allowed("/etc/passwd"));
        assert!(!path_allowed("reference/other.toml"));
    }

    #[test]
    fn round_trip_and_limits() {
        let mut files = BTreeMap::new();
        files.insert("brands/acme.toml".to_string(), "format = 1".to_string());
        let b = Bundle::new(files);
        let gz = b.to_gz();
        assert_eq!(Bundle::from_gz(&gz).unwrap(), b);
        assert!(Bundle::from_gz(b"not gzip").is_err());

        // A path that may not be in a bundle.
        let mut bad = b.clone();
        bad.files
            .insert("families/acme/manual.pdf".into(), "%PDF".into());
        assert!(
            Bundle::from_gz(&bad.to_gz())
                .unwrap_err()
                .contains("unexpected path")
        );

        // A decompression bomb: small compressed, huge unpacked.
        let mut big = b.clone();
        big.files
            .insert("brands/big.toml".into(), "a".repeat(MAX_DECOMPRESSED + 10));
        assert!(Bundle::from_gz(&big.to_gz()).is_err());
    }

    #[test]
    fn manifests() {
        let body = b"hello";
        let m = format!(
            r#"{{"format":"{MANIFEST_FORMAT}","library_format":1,"version":"2026.12.1","commit":"abc123","published":"2026-12-01","bundle":"library-2026.12.1.json.gz","sha256":"{}","size":5}}"#,
            sha256_hex(body)
        );
        let m = Manifest::parse(&m).unwrap();
        m.verify(body).unwrap();
        assert!(m.verify(b"hellp").unwrap_err().contains("checksum"));
        assert!(m.verify(b"hello!").unwrap_err().contains("bytes"));
        let evil = r#"{"format":"housewright-library-release","library_format":1,"version":"2026.12.1","commit":"abc","published":"x","bundle":"../../evil.json.gz","sha256":"00","size":5}"#;
        assert!(Manifest::parse(evil).is_err());
        let newer_format = r#"{"format":"housewright-library-release","library_format":2,"version":"2026.12.1","commit":"abc","published":"x","bundle":"library-2026.12.1.json.gz","sha256":"00","size":5}"#;
        assert!(
            Manifest::parse(newer_format)
                .unwrap_err()
                .contains("format 2")
        );
    }
}
