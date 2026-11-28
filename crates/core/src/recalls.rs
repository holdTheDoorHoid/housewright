//! US product recall checks, on the device (D-27, Phase 9).
//!
//! The device downloads the public recall list from the US Consumer Product
//! Safety Commission (CPSC Recalls API, public domain) by date range only,
//! keeps a compact copy, and compares it with the household's things here.
//! **Nothing about the household is ever sent**: the only request is
//! [`cpsc_url`], which carries two dates and the format.
//!
//! Matching is conservative (a wrong "your heater is recalled" is worse
//! than a missed long shot) and always shows its evidence:
//! - *model*: the thing's model number, written without spaces or dashes,
//!   is named in the recall (product name, model field, title or
//!   description). Unless the brand also matches, the model must be at
//!   least 6 characters.
//! - *model series*: the recall names a series the model starts with
//!   ("FTVN series" and a model "FTVN199"), and the brand matches.
//! - *brand and kind*: the brand matches and the recall is about the same
//!   kind of thing (a "Space heater" and "DREO Space Heaters"). Shown as
//!   "worth a look: check the model numbers in the recall".
//!
//! Pure apart from parsing: no clock, no storage, no network.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::ItemId;
use crate::model::Item;

/// The CPSC Recalls API (checked 2026-11-28; it sends
/// `Access-Control-Allow-Origin: *`, so a browser may call it directly).
pub const CPSC_ENDPOINT: &str = "https://www.saferproducts.gov/RestWebServices/Recall";
/// How far back the first download goes.
pub const INITIAL_YEARS: i16 = 10;
/// Later downloads start this many days before the last one ended, to
/// catch recalls published late.
pub const OVERLAP_DAYS: i64 = 14;
/// Each request covers at most a year (about 1.3 MB of JSON).
pub const WINDOW_DAYS: i64 = 366;
/// Ask the app to wait this long between requests (a polite rate).
pub const PAUSE_MS: u64 = 1500;
/// The longest description kept.
const MAX_DESCRIPTION: usize = 2000;
const MAX_SHORT: usize = 400;

/// One recall, as kept on the device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recall {
    /// The CPSC recall number ("27054").
    pub number: String,
    pub date: Date,
    pub title: String,
    /// The official recall page.
    pub url: String,
    #[serde(default)]
    pub products: Vec<String>,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub models: Vec<String>,
    /// Manufacturers, importers and distributors (name only).
    #[serde(default)]
    pub companies: Vec<String>,
    #[serde(default)]
    pub upcs: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hazard: String,
    #[serde(default)]
    pub remedy: String,
}

/// A date range to download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start: Date,
    pub end: Date,
}

/// The request for one window: dates and format, nothing else.
pub fn cpsc_url(w: Window) -> String {
    format!(
        "{CPSC_ENDPOINT}?format=json&RecallDateStart={}&RecallDateEnd={}",
        w.start, w.end
    )
}

fn add_days(d: Date, n: i64) -> Date {
    d.checked_add(Span::new().days(n)).unwrap_or(d)
}

/// What to download next: everything from ten years ago the first time
/// (`synced_to` is `None`), afterwards from a little before the last
/// download's end. Oldest first, a year at most each, so an interrupted
/// first download resumes where it stopped.
pub fn windows(synced_to: Option<Date>, today: Date) -> Vec<Window> {
    let mut start = match synced_to {
        None => today
            .checked_sub(Span::new().years(INITIAL_YEARS))
            .unwrap_or(today),
        Some(d) => add_days(d, -OVERLAP_DAYS).min(today),
    };
    let mut out = Vec::new();
    while start <= today {
        let end = add_days(start, WINDOW_DAYS - 1).min(today);
        out.push(Window { start, end });
        start = add_days(end, 1);
    }
    out
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_string()
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push('…');
        t
    }
}

fn names(v: &Value, key: &str, field: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|x| text(&x[field]))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// "Hesung Innovation Corp, dba DREO, of Clifton, New Jersey" -> the
/// company part ("Hesung Innovation Corp, dba DREO").
fn company(s: &str) -> String {
    match s.find(", of ") {
        Some(i) => s[..i].trim().to_string(),
        None => s.trim().to_string(),
    }
}

/// Parse a CPSC Recalls API response (a JSON array). Records without a
/// number, a date or an https link to the recall are skipped; nothing in
/// them is trusted as markup (the interface shows text only).
pub fn parse_cpsc(json: &str) -> Result<Vec<Recall>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("not a recall list: {e}"))?;
    let arr = v
        .as_array()
        .ok_or_else(|| "not a recall list: expected an array".to_string())?;
    let mut out = Vec::new();
    for r in arr {
        let number = text(&r["RecallNumber"]);
        let date = text(&r["RecallDate"]);
        let date: Option<Date> = date.get(..10).and_then(|d| d.parse().ok());
        let url = text(&r["URL"]);
        let (Some(date), false, true) = (date, number.is_empty(), url.starts_with("https://"))
        else {
            continue;
        };
        if number.len() > 20
            || !number
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            continue;
        }
        let mut companies: Vec<String> = ["Manufacturers", "Importers", "Distributors"]
            .iter()
            .flat_map(|k| names(r, k, "Name"))
            .map(|n| company(&n))
            .collect();
        companies.dedup();
        out.push(Recall {
            number,
            date,
            title: cut(&text(&r["Title"]), MAX_SHORT),
            url: cut(&url, 500),
            products: names(r, "Products", "Name"),
            types: names(r, "Products", "Type"),
            models: names(r, "Products", "Model"),
            companies,
            upcs: r
                .get("ProductUPCs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| match x {
                            Value::String(s) => s.trim().to_string(),
                            other => text(&other["UPC"]),
                        })
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            description: cut(&text(&r["Description"]), MAX_DESCRIPTION),
            hazard: cut(
                &names(r, "Hazards", "Name")
                    .first()
                    .cloned()
                    .unwrap_or_default(),
                MAX_SHORT,
            ),
            remedy: cut(
                &names(r, "Remedies", "Name")
                    .first()
                    .cloned()
                    .unwrap_or_default(),
                MAX_SHORT,
            ),
        });
    }
    Ok(out)
}

/// Uppercase letters and digits only: "dr-hsh015" -> "DRHSH015".
pub fn normalise(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Words as written (split on spaces and punctuation that never sits
/// inside a model number), each with its normalised form.
fn words(s: &str) -> Vec<(String, String)> {
    s.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | ':' | '(' | ')' | '[' | ']' | '"' | '“' | '”' | '‘' | '’' | '!' | '?'
            )
    })
    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
    .filter(|w| !w.is_empty())
    .map(|w| {
        let n = normalise(&w);
        (w, n)
    })
    .filter(|(_, n)| !n.is_empty())
    .collect()
}

fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

fn singular(w: &str) -> String {
    let w = w.to_ascii_uppercase();
    if let Some(s) = w.strip_suffix("IES") {
        return format!("{s}Y");
    }
    if w.ends_with("SS") {
        return w;
    }
    w.strip_suffix('S').map(str::to_string).unwrap_or(w)
}

/// Words that say nothing about the kind of product.
const GENERIC: &[&str] = &[
    "THE",
    "AND",
    "FOR",
    "WITH",
    "SET",
    "KIT",
    "UNIT",
    "UNITS",
    "MODEL",
    "PRODUCT",
    "SYSTEM",
    "ELECTRIC",
    "PORTABLE",
    "OLD",
    "NEW",
    "MAIN",
    "SMALL",
    "LARGE",
    "BIG",
    "HOME",
    "HOUSE",
    "KITCHEN",
    "BASEMENT",
    "GARAGE",
    "BATHROOM",
    "OUTSIDE",
    "INSIDE",
    "UPSTAIRS",
    "DOWNSTAIRS",
    "WATER",
    "GAS",
    "BATTERY",
    "POWER",
    "LIGHT",
];

/// The strength of a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchLevel {
    /// The model number is named in the recall.
    Model,
    /// The recall names a model series the model starts with.
    ModelSeries,
    /// Same brand and the same kind of thing: worth a look.
    BrandAndKind,
}

/// A recall that may apply to one thing, with why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallMatch {
    pub item: ItemId,
    pub number: String,
    pub level: MatchLevel,
    /// Plain sentences: what was found where.
    pub evidence: Vec<String>,
}

/// A short quote around `needle` (a normalised token) in `hay`.
fn quote(hay: &str, needle_words: usize, at: usize) -> String {
    let ws: Vec<&str> = hay.split_whitespace().collect();
    let from = at.saturating_sub(5);
    let to = (at + needle_words + 5).min(ws.len());
    let mut q = ws[from..to].join(" ");
    if from > 0 {
        q = format!("…{q}");
    }
    if to < ws.len() {
        q.push('…');
    }
    q
}

/// Where in the recall the brand appears, if it does.
fn brand_found(make: &str, r: &Recall) -> Option<String> {
    let mw: Vec<String> = words(make).into_iter().map(|(_, n)| n).collect();
    if mw.is_empty() {
        return None;
    }
    let mut keys = vec![mw.clone()];
    if mw.len() > 1 && mw[0].len() >= 3 && !GENERIC.contains(&mw[0].as_str()) {
        keys.push(vec![mw[0].clone()]);
    }
    let title_brand = r.title.find(" Recall").map(|i| &r.title[..i]).unwrap_or("");
    let places: Vec<(&str, &str)> = [("the recall's title", title_brand)]
        .into_iter()
        .chain(r.products.iter().map(|p| ("the product name", p.as_str())))
        .chain(
            r.companies
                .iter()
                .map(|c| ("the company named", c.as_str())),
        )
        .collect();
    for (where_, text) in places {
        let tw: Vec<String> = words(text).into_iter().map(|(_, n)| n).collect();
        for k in &keys {
            if !k.is_empty() && tw.windows(k.len()).any(|w| w == k.as_slice()) {
                return Some(format!("The brand “{}” is in {where_}.", make.trim()));
            }
        }
    }
    None
}

/// Every place the recall's words appear, for model look-ups.
fn texts(r: &Recall) -> Vec<(&'static str, &str)> {
    let mut v: Vec<(&'static str, &str)> = Vec::new();
    for m in &r.models {
        v.push(("the recall's model list", m));
    }
    for p in &r.products {
        v.push(("the product name", p));
    }
    v.push(("the recall's title", &r.title));
    v.push(("the recall's description", &r.description));
    v
}

fn model_found(model: &str, r: &Recall, need: usize) -> Option<(MatchLevel, String)> {
    let m = normalise(model);
    if m.len() < need || !has_digit(&m) {
        return None;
    }
    // Exact: the model, as one to three adjacent words ("S1 Pro").
    for (where_, t) in texts(r) {
        let ws = words(t);
        for n in 1..=3 {
            for (i, w) in ws.windows(n).enumerate() {
                let joined: String = w.iter().map(|(_, x)| x.as_str()).collect();
                if joined == m {
                    let original = w
                        .iter()
                        .map(|(o, _)| o.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let q = if where_ == "the recall's description" {
                        format!(": “{}”", quote(t, n, word_index(t, i)))
                    } else {
                        String::new()
                    };
                    return Some((
                        MatchLevel::Model,
                        format!(
                            "Your model “{}” is named in {where_} as “{original}”{q}.",
                            model.trim()
                        ),
                    ));
                }
            }
        }
    }
    None
}

/// The index in `t.split_whitespace()` of the `i`th word from [`words`]
/// (close enough for a quote).
fn word_index(t: &str, i: usize) -> usize {
    let mut seen = 0;
    for (k, raw) in t.split_whitespace().enumerate() {
        let n = words(raw).len();
        if seen + n > i {
            return k;
        }
        seen += n;
    }
    0
}

fn series_found(model: &str, r: &Recall) -> Option<String> {
    let m = normalise(model);
    if m.len() < 5 {
        return None;
    }
    for (where_, t) in texts(r) {
        for (orig, n) in words(t) {
            // A series is written in capitals or carries a digit, so an
            // ordinary word ("Space") is never taken for one.
            let looks_like_code =
                has_digit(&n) || (orig.chars().all(|c| !c.is_ascii_lowercase()) && n.len() >= 3);
            if n.len() >= 3
                && n.len() < m.len()
                && m.starts_with(&n)
                && looks_like_code
                && orig.chars().any(|c| c.is_ascii_alphabetic())
                && n.len() * 2 >= m.len().min(12)
            {
                return Some(format!(
                    "Your model “{}” starts with “{orig}”, which {where_} names.",
                    model.trim()
                ));
            }
        }
    }
    None
}

/// The kind words of a thing: its name and template ("space heater").
fn kind_phrases(i: &Item) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for src in [Some(i.name.as_str()), i.template.as_deref()] {
        let Some(s) = src else { continue };
        let s = s.replace(['-', '_'], " ");
        let ws: Vec<String> = words(&s)
            .into_iter()
            .map(|(_, n)| singular(&n))
            .filter(|w| w.len() >= 4 && !GENERIC.contains(&w.as_str()) && !has_digit(w))
            .collect();
        if !ws.is_empty() {
            out.push(ws);
        }
    }
    out
}

fn kind_found(i: &Item, r: &Recall) -> Option<String> {
    let phrases = kind_phrases(i);
    for (where_, t) in r
        .products
        .iter()
        .map(|p| ("the product name", p.as_str()))
        .chain(r.types.iter().map(|p| ("the product type", p.as_str())))
        .chain([("the recall's title", r.title.as_str())])
    {
        let tw: Vec<String> = words(t).into_iter().map(|(_, n)| singular(&n)).collect();
        for p in &phrases {
            if tw.windows(p.len()).any(|w| w == p.as_slice()) {
                return Some(format!(
                    "It is about the same kind of thing: “{}” in {where_}.",
                    p.join(" ").to_lowercase()
                ));
            }
        }
    }
    None
}

/// Compare one thing with one recall.
pub fn match_one(i: &Item, r: &Recall) -> Option<RecallMatch> {
    let brand = i.make.as_deref().and_then(|m| brand_found(m, r));
    let model = i.model.as_deref();
    let mut evidence = Vec::new();
    let level = if let Some((lvl, e)) =
        model.and_then(|m| model_found(m, r, if brand.is_some() { 4 } else { 6 }))
    {
        evidence.push(e);
        lvl
    } else if let (Some(_), Some(e)) = (&brand, model.and_then(|m| series_found(m, r))) {
        evidence.push(e);
        MatchLevel::ModelSeries
    } else if let (Some(_), Some(e)) = (&brand, kind_found(i, r)) {
        evidence.push(e);
        if let Some(m) = model.filter(|m| !m.trim().is_empty()) {
            evidence.push(format!(
                "Your model “{}” is not named, so check the model numbers on the recall page.",
                m.trim()
            ));
        }
        MatchLevel::BrandAndKind
    } else {
        return None;
    };
    if let Some(b) = brand {
        evidence.insert(0, b);
    } else if let Some(m) = i.make.as_deref().filter(|m| !m.trim().is_empty()) {
        evidence.push(format!(
            "The brand “{}” is not named in the recall.",
            m.trim()
        ));
    }
    Some(RecallMatch {
        item: i.id,
        number: r.number.clone(),
        level,
        evidence,
    })
}

/// Every match between these things and these recalls, strongest first,
/// leaving out the ones the household has said are not theirs. A thing
/// with neither a make nor a model is never matched.
pub fn matches(
    items: &[Item],
    recalls: &[Recall],
    dismissed: &BTreeSet<(ItemId, String)>,
) -> Vec<RecallMatch> {
    let mut out = Vec::new();
    for i in items {
        let has_make = i.make.as_deref().is_some_and(|m| !m.trim().is_empty());
        let has_model = i.model.as_deref().is_some_and(|m| !m.trim().is_empty());
        if !has_make && !has_model {
            continue;
        }
        for r in recalls {
            if dismissed.contains(&(i.id, r.number.clone())) {
                continue;
            }
            if let Some(m) = match_one(i, r) {
                out.push(m);
            }
        }
    }
    let dates: BTreeMap<&str, Date> = recalls
        .iter()
        .map(|r| (r.number.as_str(), r.date))
        .collect();
    out.sort_by(|a, b| {
        a.level.cmp(&b.level).then(
            dates
                .get(b.number.as_str())
                .cmp(&dates.get(a.number.as_str())),
        )
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn windows_first_time_and_after() {
        let today = date(2026, 11, 28);
        let w = windows(None, today);
        assert_eq!(w.first().unwrap().start, date(2016, 11, 28));
        assert_eq!(w.last().unwrap().end, today);
        assert!(w.len() >= 10 && w.len() <= 11);
        for p in w.windows(2) {
            assert_eq!(add_days(p[0].end, 1), p[1].start);
        }
        let w = windows(Some(date(2026, 11, 1)), today);
        assert_eq!(
            w,
            vec![Window {
                start: date(2026, 10, 18),
                end: today
            }]
        );
    }

    #[test]
    fn the_request_carries_dates_only() {
        let u = cpsc_url(Window {
            start: date(2025, 1, 1),
            end: date(2025, 12, 31),
        });
        assert_eq!(
            u,
            "https://www.saferproducts.gov/RestWebServices/Recall?format=json&RecallDateStart=2025-01-01&RecallDateEnd=2025-12-31"
        );
    }

    #[test]
    fn normalising() {
        assert_eq!(normalise("dr-hsh015"), "DRHSH015");
        assert_eq!(singular("Heaters"), "HEATER");
        assert_eq!(singular("Batteries"), "BATTERY");
        assert_eq!(singular("Glass"), "GLASS");
    }
}
