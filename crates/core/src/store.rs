//! SQLite storage and the operation-log apply path.
//!
//! Layout: one table per [`EntityKind`] (`id`, `data` as a JSON object,
//! `deleted`), a `field_clock` table holding the winning [`OrderKey`] for every
//! field of every entity, an `ops` table holding every operation ever seen, and
//! a small `meta` table. Every mutation goes through an [`Op`]: local changes
//! are turned into ops and applied by exactly the same code that applies ops
//! from other devices, so there is one merge path.
//!
//! JSON objects are written with sorted keys (serde_json's default map), so two
//! stores with the same state hold byte-identical rows.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::hlc::Hlc;
use crate::ids::{MemberId, OpId};
use crate::model::{Entity, EntityKind};
use crate::oplog::{DELETED_FIELD, Op, OrderKey, RECORD_FIELD};

/// Source of physical time for the HLC and for "now". Injected so tests (and
/// the property tests in particular) are deterministic.
pub trait WallClock: Send {
    fn now_ms(&self) -> u64;
}

/// The system clock.
pub struct SystemClock;

impl WallClock for SystemClock {
    fn now_ms(&self) -> u64 {
        Timestamp::now().as_millisecond().max(0) as u64
    }
}

/// A clock that only moves when told to. Clones share the same time.
#[derive(Clone, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    pub fn new(ms: u64) -> Self {
        Self(Arc::new(AtomicU64::new(ms)))
    }
    pub fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl WallClock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

type Migration = fn(&Connection) -> rusqlite::Result<()>;

/// Versioned schema migrations, applied in order, each exactly once.
const MIGRATIONS: &[(i64, Migration)] = &[
    (1, migration_1),
    (2, migration_2),
    (3, migration_3),
    (4, migration_4),
    (5, migration_5),
];

fn migration_1(conn: &Connection) -> rusqlite::Result<()> {
    let mut sql = String::from(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE ops (
             seq INTEGER PRIMARY KEY AUTOINCREMENT,
             op_id TEXT NOT NULL UNIQUE,
             order_key TEXT NOT NULL,
             hlc TEXT NOT NULL,
             author TEXT NOT NULL,
             kind TEXT NOT NULL,
             entity_id TEXT NOT NULL,
             field TEXT NOT NULL,
             value TEXT NOT NULL
         );
         CREATE INDEX ops_order ON ops(order_key);
         CREATE INDEX ops_entity ON ops(kind, entity_id);
         CREATE TABLE field_clock (
             kind TEXT NOT NULL,
             entity_id TEXT NOT NULL,
             field TEXT NOT NULL,
             order_key TEXT NOT NULL,
             PRIMARY KEY (kind, entity_id, field)
         );",
    );
    for kind in EntityKind::SCHEMA_V1 {
        sql.push_str(&entity_table(kind));
    }
    conn.execute_batch(&sql)
}

fn entity_table(kind: EntityKind) -> String {
    format!(
        "CREATE TABLE {t} (
             id TEXT PRIMARY KEY,
             data TEXT NOT NULL DEFAULT '{{}}',
             deleted INTEGER NOT NULL DEFAULT 0
         );",
        t = kind.as_str()
    )
}

/// Phase 3: the home profile and suggestion answers (D-65).
fn migration_2(conn: &Connection) -> rusqlite::Result<()> {
    let mut sql = String::new();
    for kind in [EntityKind::Profile, EntityKind::SuggestionAnswer] {
        sql.push_str(&entity_table(kind));
    }
    conn.execute_batch(&sql)
}

/// Phase 4: completion details and household template fields.
fn migration_3(conn: &Connection) -> rusqlite::Result<()> {
    let mut sql = String::new();
    for kind in [EntityKind::CompletionDetail, EntityKind::TemplateFields] {
        sql.push_str(&entity_table(kind));
    }
    conn.execute_batch(&sql)
}

/// Phase 5: house binder prints.
fn migration_4(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(&entity_table(EntityKind::BinderPrint))
}

/// Phase 6: sync state (D-95). `sync_state` holds this device's sync
/// settings and keys (never exported; see docs/FILE-FORMAT.md);
/// `sync_remote` lists the operations that arrived from other devices, so
/// they are never pushed back.
fn migration_5(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE sync_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE sync_remote (op_id TEXT PRIMARY KEY);",
    )
}

/// How far an operation's clock may be ahead of the reference time (the
/// sync server's receive time) before it is refused (D-93). Five minutes
/// absorbs ordinary clock error; anything more is a wrong clock, and
/// accepting it would drag every device's clock into the future.
pub const MAX_CLOCK_AHEAD_MS: u64 = 5 * 60 * 1000;

/// What [`Store::apply_remote_ops`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub applied: usize,
    pub duplicates: usize,
    /// Refused by the clock guard ([`Store::apply_synced_ops`] only).
    pub too_far_ahead: usize,
}

/// One row of an entity table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawEntity {
    pub id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    pub data: Map<String, Value>,
}

/// The whole materialised state, in canonical order. Two stores that have seen
/// the same ops have equal snapshots.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub entities: BTreeMap<EntityKind, Vec<RawEntity>>,
    pub clocks: Vec<(String, String, String, String)>,
}

/// The local database for one household.
pub struct Store {
    conn: Connection,
    hlc: Hlc,
    clock: Box<dyn WallClock>,
    author: Option<MemberId>,
    /// Inside [`Store::batch`]: changes join the open transaction.
    in_batch: bool,
}

impl Store {
    /// Open (or create) a database file, using the system clock.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_connection(Connection::open(path)?, Box::new(SystemClock))
    }

    /// Open (or create) a database file with a given clock.
    pub fn open_with_clock(path: impl AsRef<Path>, clock: Box<dyn WallClock>) -> Result<Self> {
        Self::with_connection(Connection::open(path)?, clock)
    }

    /// A fresh in-memory database.
    pub fn open_in_memory(clock: Box<dyn WallClock>) -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?, clock)
    }

    pub fn with_connection(conn: Connection, clock: Box<dyn WallClock>) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // WAL is best-effort (in-memory databases do not support it).
        let _ = conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()));
        let mut store = Store {
            conn,
            hlc: Hlc::ZERO,
            clock,
            author: None,
            in_batch: false,
        };
        store.migrate()?;
        if let Some(h) = store.meta("hlc")? {
            store.hlc = h.parse().map_err(Error::Invalid)?;
        }
        if let Some(a) = store.meta("local_member")? {
            store.author = Some(a.parse().map_err(|e| Error::Invalid(format!("{e}")))?);
        }
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 version INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL
             );",
        )?;
        let current: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;
        for (version, run) in MIGRATIONS {
            if *version > current {
                let tx = self.conn.transaction()?;
                run(&tx)?;
                tx.execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    params![version, Timestamp::now().to_string()],
                )?;
                tx.commit()?;
            }
        }
        Ok(())
    }

    /// The schema version this database is at.
    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// The member this device acts as. Every local op is authored by them.
    pub fn author(&self) -> Result<MemberId> {
        self.author
            .ok_or_else(|| Error::Invalid("no local member set for this database".into()))
    }

    pub fn set_author(&mut self, member: MemberId) -> Result<()> {
        self.set_meta("local_member", &member.to_string())?;
        self.author = Some(member);
        Ok(())
    }

    /// "Now" according to this store's clock.
    pub fn now(&self) -> Timestamp {
        Timestamp::from_millisecond(self.clock.now_ms() as i64).unwrap_or(Timestamp::UNIX_EPOCH)
    }

    /// The current HLC (the last one issued or observed).
    pub fn hlc(&self) -> Hlc {
        self.hlc
    }

    // -- writing ----------------------------------------------------------

    fn make_op(&mut self, kind: EntityKind, entity: &str, field: &str, value: Value) -> Result<Op> {
        let author = self.author()?;
        self.hlc = self.hlc.tick(self.clock.now_ms());
        Ok(Op {
            id: OpId::new(),
            hlc: self.hlc,
            author,
            kind,
            entity: entity.to_string(),
            field: field.to_string(),
            value,
        })
    }

    /// Apply a batch of locally created ops in one transaction.
    fn commit_local(&mut self, ops: &[Op]) -> Result<()> {
        if self.in_batch {
            for op in ops {
                apply_op(&self.conn, op)?;
            }
            self.conn.execute(
                "INSERT INTO meta (key, value) VALUES ('hlc', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [self.hlc.to_string()],
            )?;
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for op in ops {
            apply_op(&tx, op)?;
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('hlc', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [self.hlc.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Run `f` as one database transaction: every change it makes is written
    /// together, or none is if it fails. Much faster than one transaction per
    /// change where storage is slow (the browser's OPFS). Not nestable.
    pub fn batch<T>(&mut self, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        if self.in_batch {
            return f(self);
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        self.in_batch = true;
        let out = f(self);
        self.in_batch = false;
        match out {
            Ok(v) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Create or update an ordinary (last-writer-wins) entity. Emits one op per
    /// changed field; returns how many. Unchanged fields produce no ops.
    pub fn put<E: Entity>(&mut self, entity: &E) -> Result<usize> {
        if E::KIND.append_only() {
            return Err(Error::Invalid(format!(
                "{} is append-only; use append()",
                E::KIND.as_str()
            )));
        }
        let id = entity.entity_id();
        let new = to_object(entity)?;
        let old = match self.raw(E::KIND, &id)? {
            Some(r) if r.deleted => {
                return Err(Error::Invalid(format!(
                    "{} {id} has been deleted",
                    E::KIND.as_str()
                )));
            }
            Some(r) => r.data,
            None => Map::new(),
        };
        let mut ops = Vec::new();
        for (k, v) in &new {
            if old.get(k) != Some(v) {
                ops.push(self.make_op(E::KIND, &id, k, v.clone())?);
            }
        }
        for k in old.keys() {
            if !new.contains_key(k) {
                ops.push(self.make_op(E::KIND, &id, k, Value::Null)?);
            }
        }
        self.commit_local(&ops)?;
        Ok(ops.len())
    }

    /// Write an append-only record. Fails if the id already exists.
    pub fn append<E: Entity>(&mut self, record: &E) -> Result<()> {
        if !E::KIND.append_only() {
            return Err(Error::Invalid(format!(
                "{} is not append-only; use put()",
                E::KIND.as_str()
            )));
        }
        let id = record.entity_id();
        if self.raw(E::KIND, &id)?.is_some() {
            return Err(Error::Invalid(format!(
                "{} {id} already exists and cannot be changed",
                E::KIND.as_str()
            )));
        }
        let value = Value::Object(to_object(record)?);
        let op = self.make_op(E::KIND, &id, RECORD_FIELD, value)?;
        self.commit_local(&[op])
    }

    /// Delete (tombstone) an entity. Completions and snoozes cannot be deleted.
    pub fn delete(&mut self, kind: EntityKind, id: &str) -> Result<()> {
        if !kind.deletable() {
            return Err(Error::Invalid(format!(
                "{} records are never deleted; void them with a later record",
                kind.as_str()
            )));
        }
        if self.raw(kind, id)?.is_none() {
            return Err(Error::NotFound(format!("{} {id}", kind.as_str())));
        }
        let op = self.make_op(kind, id, DELETED_FIELD, Value::Bool(true))?;
        self.commit_local(&[op])
    }

    /// Write a raw entity as new local ops (used by state-only imports).
    pub(crate) fn write_raw(&mut self, kind: EntityKind, raw: &RawEntity) -> Result<()> {
        let mut ops = Vec::new();
        if kind.append_only() {
            ops.push(self.make_op(kind, &raw.id, RECORD_FIELD, Value::Object(raw.data.clone()))?);
        } else {
            for (k, v) in &raw.data {
                if k.starts_with('$') {
                    return Err(Error::Import(format!("field name '{k}' is reserved")));
                }
                ops.push(self.make_op(kind, &raw.id, k, v.clone())?);
            }
        }
        if raw.deleted {
            if !kind.deletable() {
                return Err(Error::Import(format!(
                    "{} records cannot be deleted",
                    kind.as_str()
                )));
            }
            ops.push(self.make_op(kind, &raw.id, DELETED_FIELD, Value::Bool(true))?);
        }
        self.commit_local(&ops)
    }

    /// Merge operations from another device (or an import). Idempotent and
    /// order-independent: any interleaving of the same ops converges.
    pub fn apply_remote_ops(&mut self, ops: &[Op]) -> Result<ApplyReport> {
        let mut report = ApplyReport::default();
        let now = self.clock.now_ms();
        let tx = self.conn.unchecked_transaction()?;
        for op in ops {
            self.hlc = self.hlc.observe(op.hlc, now);
            if apply_op(&tx, op)? {
                report.applied += 1;
            } else {
                report.duplicates += 1;
            }
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('hlc', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [self.hlc.to_string()],
        )?;
        tx.commit()?;
        Ok(report)
    }

    /// Merge operations that came through sync. Like
    /// [`Store::apply_remote_ops`], plus:
    ///
    /// * the clock guard (D-93): an op whose clock is more than
    ///   [`MAX_CLOCK_AHEAD_MS`] past `reference_ms` is refused and counted.
    ///   `reference_ms` must be the same for every receiver (the server's
    ///   receive time), never this device's own clock, or devices whose
    ///   clocks disagree would accept different ops and stop converging;
    /// * every newly applied op is remembered as remote, so it is never
    ///   pushed back to the server.
    pub fn apply_synced_ops(&mut self, ops: &[Op], reference_ms: u64) -> Result<ApplyReport> {
        let mut report = ApplyReport::default();
        let now = self.clock.now_ms();
        let limit = reference_ms.saturating_add(MAX_CLOCK_AHEAD_MS);
        let tx = self.conn.unchecked_transaction()?;
        for op in ops {
            if op.hlc.wall_ms > limit {
                report.too_far_ahead += 1;
                continue;
            }
            self.hlc = self.hlc.observe(op.hlc, now);
            if apply_op(&tx, op)? {
                tx.execute(
                    "INSERT OR IGNORE INTO sync_remote (op_id) VALUES (?1)",
                    [op.id.to_string()],
                )?;
                report.applied += 1;
            } else {
                report.duplicates += 1;
            }
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('hlc', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [self.hlc.to_string()],
        )?;
        tx.commit()?;
        Ok(report)
    }

    /// Operations made on this device (not received through sync), with
    /// local sequence number above `after`, oldest first, at most `limit`.
    pub fn unsynced_ops(&self, after: i64, limit: usize) -> Result<Vec<(i64, Op)>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, op_id, hlc, author, kind, entity_id, field, value FROM ops
             WHERE seq > ?1 AND op_id NOT IN (SELECT op_id FROM sync_remote)
             ORDER BY seq LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after, limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ),
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, (id, hlc, author, kind, entity, field, value)) = row?;
            out.push((
                seq,
                Op {
                    id: id.parse().map_err(|e| Error::Invalid(format!("{e}")))?,
                    hlc: hlc.parse().map_err(Error::Invalid)?,
                    author: author.parse().map_err(|e| Error::Invalid(format!("{e}")))?,
                    kind: EntityKind::parse(&kind)
                        .ok_or_else(|| Error::Invalid(format!("unknown kind {kind}")))?,
                    entity,
                    field,
                    value: serde_json::from_str(&value)?,
                },
            ));
        }
        Ok(out)
    }

    /// How many local operations above `after` still wait to be pushed.
    pub fn unsynced_count(&self, after: i64) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM ops WHERE seq > ?1 AND op_id NOT IN (SELECT op_id FROM sync_remote)",
            [after],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    /// This device's sync settings (D-95). Never part of an export.
    pub fn sync_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn sync_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn sync_remove(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sync_state WHERE key = ?1", [key])?;
        Ok(())
    }

    /// Forget every sync setting and key on this device ("stop syncing
    /// here"). The household's data stays.
    pub fn sync_forget(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM sync_state;")?;
        Ok(())
    }

    /// Run several sync-state writes and an apply in one transaction.
    pub fn sync_set_many(&self, pairs: &[(&str, &str)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (k, v) in pairs {
            tx.execute(
                "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![k, v],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // -- reading ----------------------------------------------------------

    /// A raw row, including tombstoned ones.
    pub fn raw(&self, kind: EntityKind, id: &str) -> Result<Option<RawEntity>> {
        let row: Option<(String, i64)> = self
            .conn
            .query_row(
                &format!("SELECT data, deleted FROM {} WHERE id = ?1", kind.as_str()),
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((data, deleted)) => Ok(Some(RawEntity {
                id: id.to_string(),
                deleted: deleted != 0,
                data: serde_json::from_str(&data)?,
            })),
        }
    }

    /// Every row of a kind, including tombstoned ones, sorted by id.
    pub fn raw_all(&self, kind: EntityKind) -> Result<Vec<RawEntity>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, data, deleted FROM {} ORDER BY id",
            kind.as_str()
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, data, deleted) = row?;
            out.push(RawEntity {
                id,
                deleted: deleted != 0,
                data: serde_json::from_str(&data)?,
            });
        }
        Ok(out)
    }

    /// A live (not deleted) entity. Rows that are still incomplete (some of
    /// their field ops have not arrived yet) read as absent.
    pub fn get<E: Entity>(&self, id: &str) -> Result<Option<E>> {
        match self.raw(E::KIND, id)? {
            Some(r) if !r.deleted => Ok(serde_json::from_value(Value::Object(r.data)).ok()),
            _ => Ok(None),
        }
    }

    /// Like [`Store::get`], but an error if absent.
    pub fn fetch<E: Entity>(&self, id: &str) -> Result<E> {
        self.get(id)?
            .ok_or_else(|| Error::NotFound(format!("{} {id}", E::KIND.as_str())))
    }

    /// All live, complete entities of a type, sorted by id.
    pub fn list<E: Entity>(&self) -> Result<Vec<E>> {
        self.query(
            &format!(
                "SELECT data FROM {} WHERE deleted = 0 ORDER BY id",
                E::KIND.as_str()
            ),
            [],
        )
    }

    /// Live entities whose top-level JSON `field` equals `value`.
    pub fn list_by<E: Entity>(&self, field: &str, value: &str) -> Result<Vec<E>> {
        if !crate::fields::is_key(field) {
            return Err(Error::Invalid(format!("bad field name '{field}'")));
        }
        self.query(
            &format!(
                "SELECT data FROM {} WHERE deleted = 0 AND json_extract(data, '$.{field}') = ?1 ORDER BY id",
                E::KIND.as_str()
            ),
            [value],
        )
    }

    fn query<E: Entity, P: rusqlite::Params>(&self, sql: &str, p: P) -> Result<Vec<E>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(p, |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(e) = serde_json::from_str(&row?) {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Every op, in merge order.
    pub fn ops(&self) -> Result<Vec<Op>> {
        self.ops_where("", [])
    }

    /// Ops received after local sequence number `seq` (for a future sync
    /// cursor), with the highest sequence number returned.
    pub fn ops_since(&self, seq: i64) -> Result<(Vec<Op>, i64)> {
        let max: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM ops", [], |r| r.get(0))?;
        Ok((self.ops_where("WHERE seq > ?1", [seq])?, max))
    }

    fn ops_where<P: rusqlite::Params>(&self, filter: &str, p: P) -> Result<Vec<Op>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT op_id, hlc, author, kind, entity_id, field, value FROM ops {filter} ORDER BY order_key"
        ))?;
        let rows = stmt.query_map(p, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, hlc, author, kind, entity, field, value) = row?;
            out.push(Op {
                id: id.parse().map_err(|e| Error::Invalid(format!("{e}")))?,
                hlc: hlc.parse().map_err(Error::Invalid)?,
                author: author.parse().map_err(|e| Error::Invalid(format!("{e}")))?,
                kind: EntityKind::parse(&kind)
                    .ok_or_else(|| Error::Invalid(format!("unknown kind {kind}")))?,
                entity,
                field,
                value: serde_json::from_str(&value)?,
            });
        }
        Ok(out)
    }

    /// The canonical materialised state (entities and field clocks).
    pub fn snapshot(&self) -> Result<Snapshot> {
        let mut entities = BTreeMap::new();
        for kind in EntityKind::ALL {
            entities.insert(kind, self.raw_all(kind)?);
        }
        let mut stmt = self.conn.prepare(
            "SELECT kind, entity_id, field, order_key FROM field_clock ORDER BY kind, entity_id, field",
        )?;
        let clocks = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Snapshot { entities, clocks })
    }
}

/// Serialise an entity to a JSON object with top-level nulls removed (absent
/// and null mean the same thing; removing them keeps diffs clean).
fn to_object<E: Serialize>(e: &E) -> Result<Map<String, Value>> {
    match serde_json::to_value(e)? {
        Value::Object(mut m) => {
            m.retain(|_, v| !v.is_null());
            Ok(m)
        }
        _ => Err(Error::Invalid(
            "entity did not serialise to an object".into(),
        )),
    }
}

fn clock_of(conn: &Connection, kind: &str, id: &str, field: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT order_key FROM field_clock WHERE kind = ?1 AND entity_id = ?2 AND field = ?3",
            params![kind, id, field],
            |r| r.get(0),
        )
        .optional()?)
}

fn set_clock(conn: &Connection, kind: &str, id: &str, field: &str, key: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO field_clock (kind, entity_id, field, order_key) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(kind, entity_id, field) DO UPDATE SET order_key = excluded.order_key",
        params![kind, id, field, key],
    )?;
    Ok(())
}

fn load_data(conn: &Connection, table: &str, id: &str) -> Result<Map<String, Value>> {
    let data: Option<String> = conn
        .query_row(
            &format!("SELECT data FROM {table} WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    match data {
        Some(d) => Ok(serde_json::from_str(&d)?),
        None => Ok(Map::new()),
    }
}

fn store_data(conn: &Connection, table: &str, id: &str, data: &Map<String, Value>) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO {table} (id, data) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data"
        ),
        params![id, serde_json::to_string(data)?],
    )?;
    Ok(())
}

/// Apply one op. Returns false if it had been seen before. This function is the
/// whole merge algorithm; see [`crate::oplog`] for the rules.
fn apply_op(conn: &Connection, op: &Op) -> Result<bool> {
    let op_id = op.id.to_string();
    let seen: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ops WHERE op_id = ?1)",
        [&op_id],
        |r| r.get(0),
    )?;
    if seen {
        return Ok(false);
    }
    let key = op.order_key();
    let table = op.kind.as_str();
    conn.execute(
        "INSERT INTO ops (op_id, order_key, hlc, author, kind, entity_id, field, value)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            op_id,
            key.0,
            op.hlc.to_string(),
            op.author.to_string(),
            table,
            op.entity,
            op.field,
            serde_json::to_string(&op.value)?
        ],
    )?;

    match op.field.as_str() {
        DELETED_FIELD => {
            if op.kind.deletable() {
                conn.execute(
                    &format!(
                        "INSERT INTO {table} (id, deleted) VALUES (?1, 1)
                         ON CONFLICT(id) DO UPDATE SET deleted = 1"
                    ),
                    [&op.entity],
                )?;
            }
        }
        RECORD_FIELD => {
            if op.kind.append_only()
                && let Value::Object(record) = &op.value
            {
                let current = clock_of(conn, table, &op.entity, RECORD_FIELD)?;
                // First writer wins for records: smallest order key.
                if current.is_none_or(|c| OrderKey(c) > key) {
                    let mut record = record.clone();
                    record.retain(|_, v| !v.is_null());
                    store_data(conn, table, &op.entity, &record)?;
                    set_clock(conn, table, &op.entity, RECORD_FIELD, &key.0)?;
                }
            }
        }
        f if f.starts_with('$') => {}
        field => {
            if !op.kind.append_only() {
                let current = clock_of(conn, table, &op.entity, field)?;
                if current.is_none_or(|c| OrderKey(c) < key) {
                    let mut data = load_data(conn, table, &op.entity)?;
                    if op.value.is_null() {
                        data.remove(field);
                    } else {
                        data.insert(field.to_string(), op.value.clone());
                    }
                    store_data(conn, table, &op.entity, &data)?;
                    set_clock(conn, table, &op.entity, field, &key.0)?;
                }
            }
        }
    }
    Ok(true)
}
