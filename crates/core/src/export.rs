//! Full export and lossless re-import of a household. The format is described
//! in `docs/FILE-FORMAT.md`.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ids::MemberId;
use crate::model::{EntityKind, Member, Role};
use crate::oplog::Op;
use crate::store::{RawEntity, Store};

/// The value of the `format` field.
pub const EXPORT_FORMAT: &str = "housewright-export";
/// The current export version. Version 2 (Phase 3) added the `profile` and
/// `suggestion_answer` kinds; version 3 (Phase 4) adds `completion_detail`
/// and `template_fields`. Older files import unchanged.
pub const EXPORT_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub version: u32,
    pub exported_at: Timestamp,
    pub household_id: String,
    /// Current state, by kind, sorted by id. Deleted entities are included with
    /// `"deleted": true` so the state is complete.
    pub entities: BTreeMap<EntityKind, Vec<RawEntity>>,
    /// The full operation log, in merge order. Optional: a file without it
    /// (written by hand or by another program) imports as new history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<Op>,
}

/// What an import did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    pub entities: usize,
    pub operations: usize,
    /// True if history (the operation log) was carried over.
    pub with_history: bool,
}

/// Export everything in the store.
pub fn export(store: &Store) -> Result<Export> {
    let household_id = store
        .meta("household_id")?
        .ok_or_else(|| Error::NotFound("household".into()))?;
    let snapshot = store.snapshot()?;
    Ok(Export {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        exported_at: store.now(),
        household_id,
        entities: snapshot
            .entities
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .collect(),
        operations: store.ops()?,
    })
}

pub fn to_json(export: &Export) -> Result<String> {
    Ok(serde_json::to_string_pretty(export)?)
}

pub fn from_json(text: &str) -> Result<Export> {
    let e: Export = serde_json::from_str(text)?;
    if e.format != EXPORT_FORMAT {
        return Err(Error::Import(format!(
            "not a Housewright export ('{}')",
            e.format
        )));
    }
    if e.version > EXPORT_VERSION {
        return Err(Error::Import(format!(
            "export version {} is newer than this program understands ({EXPORT_VERSION})",
            e.version
        )));
    }
    Ok(e)
}

/// Import into an empty store. With an operation log, history is replayed
/// exactly and the result is checked against the `entities` section. Without
/// one, each entity is written as new operations authored by `as_member`.
///
/// `as_member` chooses this device's member; by default the first owner.
pub fn import(store: &mut Store, e: &Export, as_member: Option<MemberId>) -> Result<ImportReport> {
    if store.meta("household_id")?.is_some() || !store.ops()?.is_empty() {
        return Err(Error::Import("import needs an empty database".into()));
    }
    let members: Vec<Member> = e
        .entities
        .get(&EntityKind::Member)
        .into_iter()
        .flatten()
        .filter(|r| !r.deleted)
        .filter_map(|r| serde_json::from_value(serde_json::Value::Object(r.data.clone())).ok())
        .collect();
    let author = match as_member {
        Some(m) => {
            if !members.iter().any(|x| x.id == m) {
                return Err(Error::Import(format!("member {m} is not in the export")));
            }
            m
        }
        None => members
            .iter()
            .find(|m| m.role == Role::Owner)
            .map(|m| m.id)
            .ok_or_else(|| Error::Import("the export has no owner member".into()))?,
    };
    store.set_meta("household_id", &e.household_id)?;
    store.set_author(author)?;
    let count = e.entities.values().map(Vec::len).sum();

    if !e.operations.is_empty() {
        store.apply_remote_ops(&e.operations)?;
        let got = store.snapshot()?.entities;
        for kind in EntityKind::ALL {
            let want = e.entities.get(&kind).map(Vec::as_slice).unwrap_or(&[]);
            let have = got.get(&kind).map(Vec::as_slice).unwrap_or(&[]);
            if want != have {
                return Err(Error::Import(format!(
                    "the operation log does not reproduce the {} entities in the file",
                    kind.as_str()
                )));
            }
        }
        return Ok(ImportReport {
            entities: count,
            operations: e.operations.len(),
            with_history: true,
        });
    }

    for (kind, rows) in &e.entities {
        for raw in rows {
            store.write_raw(*kind, raw)?;
        }
    }
    Ok(ImportReport {
        entities: count,
        operations: store.ops()?.len(),
        with_history: false,
    })
}
