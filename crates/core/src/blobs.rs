//! Content-addressed file storage for attachments (D-17, D-71).
//!
//! A blob is the bytes of a file, named by the lowercase hex SHA-256 of
//! those bytes. The same photo added twice is stored once. [`Attachment`]
//! records in the operation log refer to blobs by hash; the blobs
//! themselves are not operations and live beside the database:
//!
//! * [`DirBlobs`]: one file per blob in a folder (Android, desktop, CLI).
//! * [`SqliteBlobs`]: a separate SQLite file (the web, where the database
//!   already lives in the browser's private file system through SQLite).
//! * [`MemoryBlobs`]: tests and the in-memory web fallback.
//!
//! Everything goes through the [`BlobStore`] trait so that Phase 6 can wrap
//! any of them in encryption (D-15) without the rest of the code noticing.
//!
//! [`Attachment`]: crate::model::Attachment

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::model::Attachment;
use crate::store::Store;

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 64 lowercase hex characters.
pub fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Where attachment bytes are kept. Implementations must be idempotent:
/// putting the same bytes twice stores them once.
pub trait BlobStore: Send {
    /// Store `bytes` and return their hash.
    fn put(&mut self, bytes: &[u8]) -> Result<String>;
    /// The bytes with this hash, if stored.
    fn get(&self, hash: &str) -> Result<Option<Vec<u8>>>;
    fn has(&self, hash: &str) -> Result<bool> {
        Ok(self.get(hash)?.is_some())
    }
    /// Remove one blob. Removing a missing blob is not an error.
    fn remove(&mut self, hash: &str) -> Result<()>;
    /// Every stored hash, sorted.
    fn list(&self) -> Result<Vec<String>>;
}

fn check(hash: &str) -> Result<()> {
    if is_hash(hash) {
        Ok(())
    } else {
        Err(Error::Invalid(format!("'{hash}' is not a content hash")))
    }
}

/// Blobs held in memory.
#[derive(Default)]
pub struct MemoryBlobs(BTreeMap<String, Vec<u8>>);

impl BlobStore for MemoryBlobs {
    fn put(&mut self, bytes: &[u8]) -> Result<String> {
        let h = sha256_hex(bytes);
        self.0.entry(h.clone()).or_insert_with(|| bytes.to_vec());
        Ok(h)
    }
    fn get(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
    fn has(&self, hash: &str) -> Result<bool> {
        Ok(self.0.contains_key(hash))
    }
    fn remove(&mut self, hash: &str) -> Result<()> {
        self.0.remove(hash);
        Ok(())
    }
    fn list(&self) -> Result<Vec<String>> {
        Ok(self.0.keys().cloned().collect())
    }
}

/// One file per blob, `<dir>/<first two hex digits>/<hash>`.
pub struct DirBlobs {
    dir: PathBuf,
}

impl DirBlobs {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(io)?;
        Ok(Self { dir })
    }
    fn path(&self, hash: &str) -> PathBuf {
        self.dir.join(&hash[..2]).join(hash)
    }
}

fn io(e: std::io::Error) -> Error {
    Error::Invalid(format!("file storage: {e}"))
}

impl BlobStore for DirBlobs {
    fn put(&mut self, bytes: &[u8]) -> Result<String> {
        let h = sha256_hex(bytes);
        let p = self.path(&h);
        if p.exists() {
            return Ok(h);
        }
        let parent = p.parent().expect("blob path has a parent");
        std::fs::create_dir_all(parent).map_err(io)?;
        // Write to a temporary name first so a crash never leaves a blob
        // whose content does not match its name.
        let tmp = parent.join(format!(".{h}.tmp"));
        std::fs::write(&tmp, bytes).map_err(io)?;
        std::fs::rename(&tmp, &p).map_err(io)?;
        Ok(h)
    }
    fn get(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        check(hash)?;
        match std::fs::read(self.path(hash)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }
    fn has(&self, hash: &str) -> Result<bool> {
        check(hash)?;
        Ok(self.path(hash).exists())
    }
    fn remove(&mut self, hash: &str) -> Result<()> {
        check(hash)?;
        match std::fs::remove_file(self.path(hash)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(e)),
        }
    }
    fn list(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(io(e)),
        };
        for sub in rd {
            let sub = sub.map_err(io)?;
            if !sub.file_type().map_err(io)?.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(sub.path()).map_err(io)? {
                let name = f.map_err(io)?.file_name().to_string_lossy().to_string();
                if is_hash(&name) {
                    out.push(name);
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

/// Blobs in their own SQLite database file. On the web this file lives in
/// OPFS next to the main database (the same storage layer, D-61).
pub struct SqliteBlobs {
    conn: Mutex<Connection>,
}

impl SqliteBlobs {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_connection(Connection::open(path)?)
    }
    pub fn in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }
    fn with_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS blob (hash TEXT PRIMARY KEY, bytes BLOB NOT NULL);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl BlobStore for SqliteBlobs {
    fn put(&mut self, bytes: &[u8]) -> Result<String> {
        let h = sha256_hex(bytes);
        self.conn().execute(
            "INSERT OR IGNORE INTO blob (hash, bytes) VALUES (?1, ?2)",
            rusqlite::params![h, bytes],
        )?;
        Ok(h)
    }
    fn get(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        check(hash)?;
        Ok(self
            .conn()
            .query_row("SELECT bytes FROM blob WHERE hash = ?1", [hash], |r| {
                r.get(0)
            })
            .optional()?)
    }
    fn has(&self, hash: &str) -> Result<bool> {
        check(hash)?;
        Ok(self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM blob WHERE hash = ?1)",
            [hash],
            |r| r.get(0),
        )?)
    }
    fn remove(&mut self, hash: &str) -> Result<()> {
        check(hash)?;
        self.conn()
            .execute("DELETE FROM blob WHERE hash = ?1", [hash])?;
        Ok(())
    }
    fn list(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT hash FROM blob ORDER BY hash")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Every hash a live (not deleted) attachment refers to, thumbnails included.
pub fn referenced(store: &Store) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for a in store.list::<Attachment>()? {
        out.insert(a.sha256);
        if let Some(t) = a.thumb_sha256 {
            out.insert(t);
        }
    }
    Ok(out)
}

/// Remove blobs no live attachment refers to. Returns how many were removed.
/// Deleting an attachment is a tombstone in the log; this is what frees the
/// space afterwards.
pub fn collect_garbage(store: &Store, blobs: &mut dyn BlobStore) -> Result<usize> {
    let keep = referenced(store)?;
    let mut n = 0;
    for h in blobs.list()? {
        if !keep.contains(&h) {
            blobs.remove(&h)?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(b: &mut dyn BlobStore) {
        let h1 = b.put(b"hello").unwrap();
        let h2 = b.put(b"hello").unwrap();
        assert_eq!(h1, h2, "same bytes, same name");
        assert_eq!(
            h1,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let h3 = b.put(b"world").unwrap();
        assert_eq!(b.list().unwrap().len(), 2, "stored once each");
        assert_eq!(b.get(&h1).unwrap().unwrap(), b"hello");
        assert!(b.has(&h3).unwrap());
        b.remove(&h3).unwrap();
        b.remove(&h3).unwrap();
        assert!(!b.has(&h3).unwrap());
        assert_eq!(b.list().unwrap(), vec![h1]);
        assert!(b.get("../etc/passwd").is_err() || b.get("../etc/passwd").unwrap().is_none());
    }

    #[test]
    fn memory_blobs_dedupe() {
        exercise(&mut MemoryBlobs::default());
    }

    #[test]
    fn sqlite_blobs_dedupe() {
        exercise(&mut SqliteBlobs::in_memory().unwrap());
    }

    #[test]
    fn dir_blobs_dedupe() {
        let dir = std::env::temp_dir().join(format!("hw-blobs-{}", uuid::Uuid::now_v7()));
        let mut b = DirBlobs::open(&dir).unwrap();
        exercise(&mut b);
        assert!(
            b.get("not-a-hash").is_err(),
            "paths are never built from untrusted names"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
