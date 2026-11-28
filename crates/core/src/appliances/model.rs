//! The appliance library's file types, as its `FORMAT.md` (version 1)
//! describes them. Kept field for field in line with the library's own
//! validator (`tools/libcheck/src/model.rs` in
//! `holdTheDoorHoid/housewright-library`), so a file libcheck accepts parses
//! here and a typo is refused rather than silently ignored.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fields::FieldDef;
use crate::priority::PriorityClass;
use crate::schedule::{Schedule, WindowPolicy};
use crate::template::Condition;

/// `checked = 2026-11-28` in a file: kept as the text of the date.
pub type CheckedDate = String;

fn date_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = toml::value::Datetime::deserialize(d)?;
    match (v.date, v.time) {
        (Some(date), None) => Ok(date.to_string()),
        _ => Err(serde::de::Error::custom("expected a date like 2026-11-28")),
    }
}

fn opt_date_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    date_text(d).map(Some)
}

/// `SNAPSHOT.toml`: which published version of the library this is. Written
/// by `tools/library-sync`, never by hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotMeta {
    pub format: u32,
    /// Dotted numbers, newest is largest: `2026.11.28`, `2026.11.28.1`.
    pub version: String,
    /// The library commit the files were taken from (full hash).
    pub commit: String,
    /// The library's public repository.
    pub source: String,
    #[serde(deserialize_with = "date_text")]
    pub published: String,
    /// Licence of the data (CC BY 4.0, D-128).
    pub licence: String,
    /// The credit line the licence asks for.
    pub credit: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateList {
    pub format: u32,
    #[serde(default)]
    pub app_commit: Option<String>,
    #[serde(default)]
    pub template: Vec<TemplateRef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateRef {
    pub id: String,
    pub category: String,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub schedules: BTreeMap<String, String>,
    /// The template's meter keys, so usage overrides can be checked.
    #[serde(default)]
    pub meters: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Domains {
    pub format: u32,
    #[serde(default)]
    pub shared: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub format: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub life_safety: bool,
    #[serde(default)]
    pub brands: Vec<String>,
    pub why_brands: String,
    #[serde(default)]
    pub why_brands_source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brand {
    pub format: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub parent: Option<String>,
    pub domains: Vec<String>,
    pub website: String,
    pub support_url: String,
    #[serde(default)]
    pub manual_search_url: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub unverified: bool,
    /// Brand ids of a different company printed with the same name on labels
    /// (e.g. Bosch appliances vs. Bosch Thermotechnology). Matching keeps both
    /// and relies on the kind of thing to tell them apart.
    #[serde(default)]
    pub same_label_as: Vec<String>,
    #[serde(default)]
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(deserialize_with = "date_text")]
    pub checked: CheckedDate,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub format: u32,
    pub id: String,
    pub brand: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub fuel: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub years: Option<String>,
    #[serde(default)]
    pub model_patterns: Vec<String>,
    /// Same syntax as `model_patterns`, but not confirmed by the maker: a
    /// match is only ever a suggestion to check, never "matches".
    #[serde(default)]
    pub unconfirmed_patterns: Vec<String>,
    /// Official pages documenting the model-number patterns (each also in `sources`).
    #[serde(default)]
    pub pattern_sources: Vec<String>,
    #[serde(default)]
    pub model_examples: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub manuals: Vec<Manual>,
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub consumables: Vec<Consumable>,
    #[serde(default)]
    pub tasks: Vec<TaskOverride>,
    #[serde(default)]
    pub warranty: Option<Warranty>,
    #[serde(default)]
    pub recalls: Vec<Recall>,
    #[serde(default)]
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manual {
    /// manual-search | product-page | support-page | manual-pdf | parts-page
    pub kind: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub topic: String,
    pub text: String,
    pub source: String,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consumable {
    pub name: String,
    #[serde(default)]
    pub part: Option<String>,
    /// Other part numbers the maker lists for the same thing.
    #[serde(default)]
    pub alternates: Vec<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub replace_every: Option<String>,
    #[serde(default)]
    pub for_task: Option<String>,
    pub source: String,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

/// A family's `[[tasks]]` entry: a partial override of a generic task (only
/// what differs), or a whole new task.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskOverride {
    pub key: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub priority: Option<PriorityClass>,
    #[serde(default)]
    pub schedule: Option<Schedule>,
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub steps: Option<Vec<String>>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub window: Option<WindowPolicy>,
    #[serde(default)]
    pub when: Option<Condition>,
    #[serde(default)]
    pub completion_fields: Option<Vec<FieldDef>>,
    pub source: String,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warranty {
    pub summary: String,
    #[serde(default)]
    pub length: Option<String>,
    pub source: String,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recall {
    pub title: String,
    #[serde(default, deserialize_with = "opt_date_text")]
    pub date: Option<CheckedDate>,
    pub url: String,
    #[serde(default)]
    pub unverified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

impl Family {
    /// Every URL this family points at, with what kind of reference it is.
    pub fn urls(&self) -> Vec<(&'static str, &str)> {
        let mut v = Vec::new();
        for m in &self.manuals {
            v.push(("manual", m.url.as_str()));
            if let Some(s) = &m.source {
                v.push(("manual source", s.as_str()));
            }
        }
        for f in &self.facts {
            v.push(("fact source", f.source.as_str()));
        }
        for c in &self.consumables {
            v.push(("consumable source", c.source.as_str()));
        }
        for t in &self.tasks {
            v.push(("task source", t.source.as_str()));
        }
        if let Some(w) = &self.warranty {
            v.push(("warranty source", w.source.as_str()));
        }
        for r in &self.recalls {
            v.push(("recall", r.url.as_str()));
        }
        v
    }

    /// When the page behind `url` was last checked, from `[[sources]]`.
    pub fn checked(&self, url: &str) -> Option<&str> {
        self.sources
            .iter()
            .find(|s| s.url == url)
            .map(|s| s.checked.as_str())
    }
}
