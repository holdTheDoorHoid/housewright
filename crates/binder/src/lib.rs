//! The house binder (D-40 to D-43, D-80 to D-85): a printable house manual
//! that stands on its own without the app.
//!
//! ```text
//! Store ─► data::gather ─► HouseData ──────────────► build::sealed_volume ─► Document (sealed)
//!                              └─► redact::Public ─► build::main_volume ───► Document (main)
//! Document ─► typeset::source (Typst, all user text as string literals) ─► render ─► PDF
//! Document ─► Section::fingerprint ─► track::status ("3 sections changed")
//! ```
//!
//! Licensed AGPL-3.0-only.

pub mod build;
pub mod bundle;
pub mod data;
pub mod doc;
pub mod redact;
#[cfg(feature = "typeset")]
pub mod render;
pub mod text;
pub mod track;
pub mod typeset;

use std::collections::BTreeMap;

pub use build::Options;
pub use data::{HouseData, Recovery, SEALED_LOCATION_KEY, gather};
pub use doc::{Document, Edition, Volume};
pub use redact::Public;
pub use typeset::Paper;

use housewright_core::model::Sensitivity;

/// The binder could not be made.
#[derive(Debug, thiserror::Error)]
#[error("the binder could not be typeset: {0}")]
pub struct RenderError(pub String);

/// A typeset document.
pub struct Rendered {
    pub pdf: Vec<u8>,
    pub pages: usize,
    /// The laid-out text of each page, exactly as it is drawn into the PDF
    /// (one line per run of text). For checks and tests.
    pub page_text: Vec<String>,
}

/// Both volumes of one edition.
#[derive(Clone, Debug)]
pub struct Binder {
    pub main: Document,
    /// `None` for editions without a sealed volume (emergency, handover).
    pub sealed: Option<Document>,
}

/// Build an edition. The main volume only ever sees the redacted data.
pub fn build(full: &HouseData, edition: Edition, opts: Options) -> Binder {
    let public = Public::of(full);
    Binder {
        main: build::main_volume(&public, edition, opts),
        sealed: build::sealed_volume(full, edition, opts),
    }
}

/// Sensitive values worth checking for, as plain strings. Short or
/// ordinary words ("Yes", "Basement") are left out: they would also
/// appear in normal text.
pub fn sensitive_strings(full: &HouseData) -> Vec<String> {
    let s = |x: Sensitivity| x == Sensitivity::Sensitive;
    let mut v: Vec<String> = Vec::new();
    for f in &full.facts {
        if s(f.fact.sensitivity) {
            v.push(f.fact.value.clone());
        }
    }
    for it in &full.items {
        let i = &it.item;
        let whole = s(i.sensitivity);
        if let Some(x) = &i.serial
            && (whole || s(x.sensitivity))
        {
            v.push(x.value.clone());
        }
        if let Some(x) = &i.location_note
            && (whole || s(x.sensitivity))
        {
            v.push(x.value.clone());
        }
        if let Some(w) = &i.warranty
            && let Some(r) = &w.reference
            && (whole || s(r.sensitivity))
        {
            v.push(r.value.clone());
        }
        for (k, val) in &i.custom {
            let def = it.fields.iter().any(|f| &f.key == k && s(f.sensitivity));
            if (whole || def || s(val.sensitivity))
                && let Some(t) = text::field_value(&val.value)
            {
                v.push(t);
            }
        }
    }
    for n in &full.notes {
        if s(n.text.sensitivity) {
            v.push(n.text.value.clone());
        }
    }
    for c in &full.contractors {
        if let Some(n) = &c.notes
            && s(n.sensitivity)
        {
            v.push(n.value.clone());
        }
        for x in [&c.licence, &c.insurance].into_iter().flatten() {
            if s(x.number.sensitivity) {
                v.push(x.number.value.clone());
            }
        }
    }
    if let Some(a) = &full.property.address
        && s(a.sensitivity)
    {
        v.push(a.value.clone());
    }
    v.retain(|x| {
        let t = x.trim();
        let n = t.chars().count();
        (n >= 6 && t.chars().any(|c| c.is_ascii_digit())) || n >= 12
    });
    v
}

/// A last check before printing a main volume: none of the household's
/// sensitive values may appear in it. The redaction should make this
/// impossible; if it ever fires, nothing is printed.
pub fn check_main(full: &HouseData, main: &Document) -> Result<(), RenderError> {
    let texts = main.texts();
    for secret in sensitive_strings(full) {
        let t = secret.trim();
        if texts.iter().any(|x| x.contains(t)) {
            return Err(RenderError(
                "something marked sensitive would have been printed in the main binder, so nothing was printed. Please report this as a bug".into(),
            ));
        }
    }
    Ok(())
}

/// Everything needed to typeset `doc`: the Typst source, the photos it
/// uses (`images` maps a photo's `file` to scaled JPEG bytes; a photo
/// missing from it is left out) and the receipt to record.
pub fn prepare(
    doc: &Document,
    property: housewright_core::ids::PropertyId,
    printed: jiff::civil::Date,
    paper: Paper,
    images: &BTreeMap<String, Vec<u8>>,
) -> bundle::Bundle {
    // Fingerprints of the document as built, before photos that could not
    // be found are dropped, so tracking does not depend on the device.
    let sections = doc
        .sections
        .iter()
        .map(|s| (s.key.clone(), s.fingerprint()))
        .collect();
    let mut doc = doc.clone();
    for s in &mut doc.sections {
        for b in &mut s.blocks {
            match b {
                doc::Block::Photos { photos } | doc::Block::Emergency { photos, .. } => {
                    photos.retain(|p| images.contains_key(&p.file));
                }
                _ => {}
            }
        }
        s.blocks
            .retain(|b| !matches!(b, doc::Block::Photos { photos } if photos.is_empty()));
    }
    let files = doc
        .photo_files()
        .into_iter()
        .filter_map(|f| images.get(&f).map(|b| (f, b.clone())))
        .collect();
    bundle::Bundle {
        source: typeset::source(&doc, &text::date(printed), paper),
        files,
        receipt: bundle::Receipt {
            property,
            edition: doc.edition.as_str().into(),
            volume: doc.volume.as_str().into(),
            printed_on: printed,
            sections,
        },
    }
}

/// Typeset a document (in this process; needs the `typeset` feature).
#[cfg(feature = "typeset")]
pub fn pdf(
    doc: &Document,
    printed: jiff::civil::Date,
    paper: Paper,
    images: &BTreeMap<String, Vec<u8>>,
) -> Result<Rendered, RenderError> {
    let b = prepare(doc, Default::default(), printed, paper, images);
    render::render(&b.source, &b.files, printed)
}
