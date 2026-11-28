//! The audit view: a readable projection of the operation log
//! ("Alex marked 'Replace furnace filter' done on 2026-11-27").
//!
//! Values are never printed, only which fields changed, so sensitive values
//! (D-41) cannot leak into the audit view.

use std::collections::BTreeSet;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;
use serde_json::Value;

use crate::error::Result;
use crate::ids::MemberId;
use crate::model::EntityKind;
use crate::oplog::{DELETED_FIELD, Op, RECORD_FIELD};
use crate::schedule::civil_date;
use crate::store::Store;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuditEntry {
    pub at: Timestamp,
    pub author: MemberId,
    pub text: String,
}

fn noun(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::MeterReading => "meter reading",
        k => k.as_str(),
    }
}

/// The audit trail, oldest first. Consecutive ops by the same author on the
/// same entity (one save) are one entry. Dates are shown in `tz`.
pub fn audit_log(store: &Store, tz: &TimeZone) -> Result<Vec<AuditEntry>> {
    let ops = store.ops()?;
    let mut seen: BTreeSet<(EntityKind, String)> = BTreeSet::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        let first = &ops[i];
        let mut j = i + 1;
        while j < ops.len()
            && ops[j].author == first.author
            && ops[j].kind == first.kind
            && ops[j].entity == first.entity
            && !ops[j].field.starts_with('$')
            && !first.field.starts_with('$')
        {
            j += 1;
        }
        let group = &ops[i..j];
        let created = seen.insert((first.kind, first.entity.clone()));
        let text = describe(store, group, created, tz);
        out.push(AuditEntry {
            at: Timestamp::from_millisecond(first.hlc.wall_ms as i64).unwrap_or_default(),
            author: first.author,
            text,
        });
        i = j;
    }
    Ok(out)
}

fn label(store: &Store, kind: EntityKind, id: &str) -> String {
    let data = match store.raw(kind, id) {
        Ok(Some(r)) => r.data,
        _ => return format!("{} {}", noun(kind), short(id)),
    };
    for key in ["title", "name", "label"] {
        if let Some(Value::String(s)) = data.get(key) {
            return format!("{} '{s}'", noun(kind));
        }
    }
    format!("{} {}", noun(kind), short(id))
}

fn title_of_task(store: &Store, task: Option<&Value>) -> String {
    let Some(Value::String(id)) = task else {
        return "a task".into();
    };
    match store.raw(EntityKind::Task, id) {
        Ok(Some(r)) => match r.data.get("title") {
            Some(Value::String(t)) => format!("'{t}'"),
            _ => format!("task {}", short(id)),
        },
        _ => format!("task {}", short(id)),
    }
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

fn date_of(v: Option<&Value>, tz: &TimeZone) -> String {
    v.and_then(Value::as_str)
        .and_then(|s| s.parse::<Timestamp>().ok())
        .map(|t| civil_date(t, tz).to_string())
        .unwrap_or_else(|| "an unknown date".into())
}

fn describe(store: &Store, group: &[Op], created: bool, tz: &TimeZone) -> String {
    let first = &group[0];
    let who = store.member_name(first.author);
    let kind = first.kind;
    match first.field.as_str() {
        DELETED_FIELD => format!("{who} deleted {}", label(store, kind, &first.entity)),
        RECORD_FIELD => {
            let r = &first.value;
            match kind {
                EntityKind::Completion => {
                    let task = title_of_task(store, r.get("task"));
                    if r.get("voids").is_some_and(|v| !v.is_null()) {
                        format!("{who} cancelled a completion of {task} logged by mistake")
                    } else {
                        format!("{who} marked {task} done on {}", date_of(r.get("at"), tz))
                    }
                }
                EntityKind::Snooze => format!(
                    "{who} snoozed {} until {}",
                    title_of_task(store, r.get("task")),
                    r.get("until").and_then(Value::as_str).unwrap_or("?")
                ),
                EntityKind::MeterReading => {
                    let meter = r
                        .get("meter")
                        .and_then(Value::as_str)
                        .map(|m| label(store, EntityKind::Meter, m))
                        .unwrap_or_else(|| "a meter".into());
                    let value = r.get("value").and_then(Value::as_f64).unwrap_or(f64::NAN);
                    format!("{who} recorded a reading of {value} on {meter}")
                }
                EntityKind::Attachment => {
                    format!("{who} attached {}", label(store, kind, &first.entity))
                }
                EntityKind::Note => format!("{who} added a note"),
                _ => format!("{who} wrote {}", label(store, kind, &first.entity)),
            }
        }
        _ => {
            let what = label(store, kind, &first.entity);
            if created {
                format!("{who} added {what}")
            } else {
                if kind == EntityKind::Task && group.len() == 1 && first.field == "active" {
                    let state = if first.value == Value::Bool(true) {
                        "on"
                    } else {
                        "off"
                    };
                    return format!("{who} switched {state} {what}");
                }
                let fields: Vec<String> = group.iter().map(|o| o.field.replace('_', " ")).collect();
                format!("{who} changed the {} of {what}", fields.join(", "))
            }
        }
    }
}
