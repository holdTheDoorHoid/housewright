//! "3 sections changed since you printed on Nov 2" (D-42, D-82).
//!
//! Each print is recorded with a fingerprint per section. A section has
//! changed when its fingerprint now differs from the one in the latest
//! print that included it, or when it has never been printed.

use housewright_core::model::BinderPrint;
use jiff::civil::Date;
use serde::Serialize;

use crate::doc::{Document, Edition};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SectionStatus {
    pub key: String,
    pub volume: String,
    pub short: String,
    pub title: String,
    /// The section's content differs from its last print (or it was never
    /// printed).
    pub changed: bool,
    pub last_printed: Option<Date>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditionStatus {
    pub edition: Edition,
    /// The most recent print of any section of this edition.
    pub last_printed: Option<Date>,
    pub sections: Vec<SectionStatus>,
    pub changed: usize,
}

/// Compare the current documents with the prints recorded for this edition.
pub fn status(edition: Edition, docs: &[&Document], prints: &[BinderPrint]) -> EditionStatus {
    let mut mine: Vec<&BinderPrint> = prints
        .iter()
        .filter(|p| p.edition == edition.as_str())
        .collect();
    mine.sort_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    let mut sections = Vec::new();
    for d in docs {
        for s in &d.sections {
            let fp = s.fingerprint();
            let last = mine
                .iter()
                .rev()
                .find(|p| p.volume == d.volume.as_str() && p.sections.contains_key(&s.key));
            sections.push(SectionStatus {
                key: s.key.clone(),
                volume: d.volume.as_str().into(),
                short: s.short.clone(),
                title: s.title.clone(),
                changed: last.is_none_or(|p| p.sections[&s.key] != fp),
                last_printed: last.map(|p| p.printed_on),
                fingerprint: fp,
            });
        }
    }
    EditionStatus {
        edition,
        last_printed: mine.last().map(|p| p.printed_on),
        changed: sections.iter().filter(|s| s.changed).count(),
        sections,
    }
}
