//! Everything the typesetter needs for one PDF, in one file (D-80).
//!
//! On Android and the desktop the binder is typeset in the same process.
//! On the web the typesetter is a separate WebAssembly module, loaded only
//! when someone prints, so the app itself stays small: the main module makes
//! a *bundle* (a ZIP of the Typst source, the photos and a receipt), the
//! binder module turns it into a PDF, and the receipt is recorded as the
//! print.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use housewright_core::ids::PropertyId;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::RenderError;

/// What is recorded when a bundle is printed (D-82).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub property: PropertyId,
    pub edition: String,
    pub volume: String,
    pub printed_on: Date,
    /// Section key to fingerprint.
    pub sections: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub source: String,
    /// `img/<sha256>.jpg` to JPEG bytes.
    pub files: BTreeMap<String, Vec<u8>>,
    pub receipt: Receipt,
}

const SOURCE: &str = "main.typ";
const RECEIPT: &str = "receipt.json";
/// The most a bundle may unpack to (photos are about 100 KB each).
const MAX_TOTAL: u64 = 200 * 1024 * 1024;

fn bad(e: impl std::fmt::Display) -> RenderError {
    RenderError(format!("bad binder bundle: {e}"))
}

impl Bundle {
    pub fn to_zip(&self) -> Result<Vec<u8>, RenderError> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default();
        w.start_file(SOURCE, deflated).map_err(bad)?;
        w.write_all(self.source.as_bytes()).map_err(bad)?;
        w.start_file(RECEIPT, deflated).map_err(bad)?;
        w.write_all(&serde_json::to_vec(&self.receipt).map_err(bad)?)
            .map_err(bad)?;
        for (name, bytes) in &self.files {
            w.start_file(name.as_str(), stored).map_err(bad)?;
            w.write_all(bytes).map_err(bad)?;
        }
        Ok(w.finish().map_err(bad)?.into_inner())
    }

    pub fn from_zip(bytes: &[u8]) -> Result<Bundle, RenderError> {
        let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(bad)?;
        let mut source = None;
        let mut receipt = None;
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..z.len() {
            let mut f = z.by_index(i).map_err(bad)?;
            let name = f.name().map_err(bad)?.to_string();
            total += f.size();
            if total > MAX_TOTAL {
                return Err(bad("too large"));
            }
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(bad)?;
            match name.as_str() {
                SOURCE => source = Some(String::from_utf8(buf).map_err(bad)?),
                RECEIPT => receipt = Some(serde_json::from_slice(&buf).map_err(bad)?),
                n if is_image_name(n) => {
                    files.insert(name, buf);
                }
                n => return Err(bad(format!("unexpected file {n}"))),
            }
        }
        Ok(Bundle {
            source: source.ok_or_else(|| bad("no source"))?,
            receipt: receipt.ok_or_else(|| bad("no receipt"))?,
            files,
        })
    }
}

/// `img/` and 64 hex characters and `.jpg`: the only names a photo has.
pub fn is_image_name(n: &str) -> bool {
    n.strip_prefix("img/")
        .and_then(|r| r.strip_suffix(".jpg"))
        .is_some_and(housewright_core::blobs::is_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_refuse_strangers() {
        let b = Bundle {
            source: "#hw-p(\"x\")".into(),
            files: BTreeMap::from([(format!("img/{}.jpg", "a".repeat(64)), vec![1, 2, 3])]),
            receipt: Receipt {
                property: PropertyId::new(),
                edition: "full".into(),
                volume: "main".into(),
                printed_on: jiff::civil::date(2026, 11, 28),
                sections: BTreeMap::from([("facts".into(), "abc".into())]),
            },
        };
        assert_eq!(Bundle::from_zip(&b.to_zip().unwrap()).unwrap(), b);
        assert!(!is_image_name("img/../etc/passwd.jpg"));
        assert!(!is_image_name("main.typ"));
    }
}
