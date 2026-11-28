//! The full backup (D-73): one ZIP file holding the JSON export and every
//! attachment's bytes, so photos and manuals survive a move to a new device.
//!
//! Layout (see `docs/FILE-FORMAT.md`):
//!
//! ```text
//! housewright-export.json   the ordinary JSON export (unchanged format)
//! blobs/<sha256>            one file per stored blob, named by its hash
//! README.txt                a short note for whoever opens the zip
//! ```
//!
//! A plain JSON export still works alone; it simply has no file contents.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::blobs::{self, BlobStore, sha256_hex};
use crate::error::{Error, Result};
use crate::export::{self, ImportReport};
use crate::store::Store;

pub const EXPORT_ENTRY: &str = "housewright-export.json";
const README: &str = "This is a Housewright full backup.\n\n\
housewright-export.json holds everything you recorded, in the documented\n\
format (docs/FILE-FORMAT.md in the Housewright source). The blobs folder holds\n\
your photos and documents; each file is named by the SHA-256 of its contents,\n\
which is how the export refers to it (the \"sha256\" of an attachment).\n\
To restore, start Housewright on a fresh install and choose \"Restore a backup\".\n";

fn zerr(e: zip::result::ZipError) -> Error {
    Error::Import(format!("zip: {e}"))
}

fn ioerr(e: std::io::Error) -> Error {
    Error::Import(format!("zip: {e}"))
}

/// Write the full backup. Only blobs some live attachment uses are included.
pub fn write(store: &Store, blobs: &dyn BlobStore) -> Result<Vec<u8>> {
    let e = export::export(store)?;
    let json = export::to_json(&e)?;
    let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
    let text = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    // Photos and PDFs are already compressed: store them as they are.
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zw.start_file(EXPORT_ENTRY, text).map_err(zerr)?;
    zw.write_all(json.as_bytes()).map_err(ioerr)?;
    zw.start_file("README.txt", text).map_err(zerr)?;
    zw.write_all(README.as_bytes()).map_err(ioerr)?;
    for h in blobs::referenced(store)? {
        if let Some(bytes) = blobs.get(&h)? {
            zw.start_file(format!("blobs/{h}"), stored).map_err(zerr)?;
            zw.write_all(&bytes).map_err(ioerr)?;
        }
    }
    Ok(zw.finish().map_err(zerr)?.into_inner())
}

/// What a restore did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreReport {
    pub import: ImportReport,
    pub blobs: usize,
    /// Attachments whose bytes were not in the backup.
    pub missing: usize,
}

/// Whether `bytes` look like a zip file (a full backup) rather than JSON.
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// Restore a full backup into an empty store. Every blob is checked against
/// its name; one whose bytes do not match is refused.
pub fn restore(
    store: &mut Store,
    blobs: &mut dyn BlobStore,
    bytes: &[u8],
) -> Result<RestoreReport> {
    let mut za = ZipArchive::new(Cursor::new(bytes)).map_err(zerr)?;
    let json = {
        let mut f = za
            .by_name(EXPORT_ENTRY)
            .map_err(|_| Error::Import(format!("this zip has no {EXPORT_ENTRY}")))?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(ioerr)?;
        s
    };
    let e = export::from_json(&json)?;
    // Check every blob before writing anything.
    let mut files = Vec::new();
    for i in 0..za.len() {
        let mut f = za.by_index(i).map_err(zerr)?;
        let full = f.name().map_err(zerr)?.to_string();
        let Some(name) = full.strip_prefix("blobs/").map(str::to_string) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        if !blobs::is_hash(&name) {
            return Err(Error::Import(format!("unexpected file blobs/{name}")));
        }
        let mut b = Vec::new();
        f.read_to_end(&mut b).map_err(ioerr)?;
        if sha256_hex(&b) != name {
            return Err(Error::Import(format!(
                "blobs/{name} does not match its name; the backup is damaged"
            )));
        }
        files.push(b);
    }
    let import = export::import(store, &e, None)?;
    for b in &files {
        blobs.put(b)?;
    }
    let missing = blobs::referenced(store)?
        .into_iter()
        .filter(|h| !blobs.has(h).unwrap_or(false))
        .count();
    Ok(RestoreReport {
        import,
        blobs: files.len(),
        missing,
    })
}
