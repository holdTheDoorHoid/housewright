//! Checks on the library's files, the same rules as the library's own
//! validator (libcheck) that matter to the app, plus the ones only the app
//! can make: every family must extend one of the app's own templates, and a
//! task override must name a task that template has or be a whole new task.
//!
//! Only errors are reported; libcheck's style warnings are the library's
//! business.

use std::collections::BTreeSet;

use crate::schedule::Interval;
use crate::template::{Library as Templates, MAX_STEPS, MAX_WHY_CHARS, MIN_STEPS};

use super::model::*;

const STATUSES: &[&str] = &["current", "discontinued", "unknown"];
const FUELS: &[&str] = &[
    "gas",
    "propane",
    "electric",
    "oil",
    "battery",
    "hardwired",
    "manual",
];
pub const MANUAL_KINDS: &[&str] = &[
    "manual-search",
    "product-page",
    "support-page",
    "manual-pdf",
    "parts-page",
];
const TRACKING_PARAMS: &[&str] = &[
    "utm_", "gclid", "fbclid", "msclkid", "mc_cid", "mc_eid", "_ga", "srsltid",
];

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The host of an `https://` URL, lowercased, or why it is not acceptable.
/// Deliberately strict (no user names, ports, odd characters): these links
/// are opened and, for manuals, downloaded.
pub fn https_host(raw: &str) -> Result<String, String> {
    let Some(rest) = raw.strip_prefix("https://") else {
        return Err("must use https://".into());
    };
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("contains spaces".into());
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    if host.is_empty() {
        return Err("has no host".into());
    }
    if host.contains('@') {
        return Err("contains credentials".into());
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        || !host.contains('.')
        || host.starts_with('.')
        || host.ends_with('.')
        || host.contains("..")
    {
        return Err(format!("has an unusual host '{host}'"));
    }
    let query = rest[end..].split_once('?').map(|(_, q)| q).unwrap_or("");
    let query = query.split('#').next().unwrap_or("");
    for pair in query.split('&') {
        let k = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
        if TRACKING_PARAMS
            .iter()
            .any(|t| !k.is_empty() && k.starts_with(t))
        {
            return Err(format!("has a tracking parameter '{k}'"));
        }
    }
    Ok(host.to_ascii_lowercase())
}

/// Is `host` one of `domains` or a subdomain of one?
pub fn host_allowed(host: &str, domains: &[String]) -> bool {
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    domains.iter().any(|d| {
        let d = d.trim_start_matches("www.").to_ascii_lowercase();
        host == d || host.ends_with(&format!(".{d}"))
    })
}

/// An https URL on one of `domains` (any host when `None`).
pub fn url_problem(raw: &str, domains: Option<&[String]>) -> Option<String> {
    match https_host(raw) {
        Err(e) => Some(format!("'{raw}' {e}")),
        Ok(host) => match domains {
            Some(d) if !host_allowed(&host, d) => Some(format!(
                "'{raw}' is on '{host}', not one of the brand's domains or a shared domain"
            )),
            _ => None,
        },
    }
}

fn text_problem(s: &str, max: usize) -> Option<String> {
    if s.trim().is_empty() {
        return Some("is empty".into());
    }
    if s.contains('\n') {
        return Some("must be one line".into());
    }
    if s.chars().count() > max {
        return Some(format!("is longer than {max} characters"));
    }
    let lower = s.to_ascii_lowercase();
    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        return Some("contains a URL".into());
    }
    None
}

/// What the checks found, one line per problem, each naming its file.
#[derive(Default, Debug)]
pub struct Problems(pub Vec<String>);

impl Problems {
    fn add(&mut self, file: &str, msg: impl AsRef<str>) {
        self.0.push(format!("{file}: {}", msg.as_ref()));
    }
    fn url(&mut self, file: &str, what: &str, raw: &str, domains: Option<&[String]>) {
        if let Some(p) = url_problem(raw, domains) {
            self.add(file, format!("{what} {p}"));
        }
    }
    fn text(&mut self, file: &str, what: &str, s: &str, max: usize) {
        if let Some(p) = text_problem(s, max) {
            self.add(file, format!("{what} {p}"));
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub(super) fn check_category(p: &mut Problems, file: &str, c: &Category, templates: &Templates) {
    if c.format != 1 {
        p.add(file, "format must be 1");
    }
    if !valid_id(&c.id) {
        p.add(file, format!("id '{}' is not valid", c.id));
    }
    p.text(file, "name", &c.name, 80);
    if let Some(t) = &c.template
        && templates.get(t).is_none()
    {
        p.add(
            file,
            format!("template '{t}' is not one of the app's templates"),
        );
    }
}

pub(super) fn check_brand(p: &mut Problems, file: &str, b: &Brand, shared: &[String]) {
    if b.format != 1 {
        p.add(file, "format must be 1");
    }
    if !valid_id(&b.id) {
        p.add(file, format!("id '{}' is not valid", b.id));
    }
    p.text(file, "name", &b.name, 80);
    if b.domains.is_empty() {
        p.add(file, "domains must list at least one official domain");
    }
    for d in &b.domains {
        if d.contains('/')
            || d.contains(':')
            || !d.contains('.')
            || d.chars()
                .any(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'))
        {
            p.add(file, format!("domain '{d}' must be a bare host name"));
        }
    }
    let allowed: Vec<String> = b.domains.iter().chain(shared).cloned().collect();
    p.url(file, "website", &b.website, Some(&allowed));
    p.url(file, "support_url", &b.support_url, Some(&allowed));
    if let Some(u) = &b.manual_search_url {
        p.url(file, "manual_search_url", u, Some(&allowed));
    }
    if let Some(ph) = &b.phone {
        let digits = ph.chars().filter(char::is_ascii_digit).count();
        if !(10..=15).contains(&digits)
            || ph
                .chars()
                .any(|c| !(c.is_ascii_digit() || " +-().".contains(c)))
        {
            p.add(file, format!("phone '{ph}' is not a phone number"));
        }
    }
    for s in &b.sources {
        p.url(file, "source", &s.url, Some(&allowed));
    }
    for a in std::iter::once(&b.name).chain(&b.aliases) {
        if super::normalise_brand(a).len() < 2 {
            p.add(
                file,
                format!("alias '{a}' is too short to match on a label"),
            );
        }
    }
}

/// The checks for one family. `brand` and `category` are looked up by the
/// caller (their absence is reported there).
pub(super) fn check_family(
    p: &mut Problems,
    file: &str,
    f: &Family,
    brand: &Brand,
    category: &Category,
    shared: &[String],
    templates: &Templates,
) {
    if f.format != 1 {
        p.add(file, "format must be 1");
    }
    if !valid_id(&f.id) {
        p.add(file, format!("id '{}' is not valid", f.id));
    }
    p.text(file, "name", &f.name, 120);
    if let Some(d) = &f.description {
        p.text(file, "description", d, 300);
    }
    if !STATUSES.contains(&f.status.as_str()) {
        p.add(file, format!("status '{}' is not known", f.status));
    }
    for fu in &f.fuel {
        if !FUELS.contains(&fu.as_str()) {
            p.add(file, format!("fuel '{fu}' is not known"));
        }
    }
    if !category.brands.contains(&f.brand) {
        p.add(
            file,
            format!("brand '{}' is not in category '{}'", f.brand, category.id),
        );
    }
    let extends = f.extends.clone().or_else(|| category.template.clone());
    let parent = match &extends {
        None => None,
        Some(t) => match templates.resolve(t) {
            Ok(r) => Some(r),
            Err(_) => {
                p.add(
                    file,
                    format!("extends '{t}', which is not one of the app's templates"),
                );
                None
            }
        },
    };
    for pat in &f.model_patterns {
        if let Some(why) = super::pattern_problem(pat) {
            p.add(file, format!("model pattern '{pat}' {why}"));
        }
    }

    let allowed: Vec<String> = brand.domains.iter().chain(shared).cloned().collect();
    for (what, u) in f.urls() {
        p.url(file, what, u, Some(&allowed));
    }
    for s in &f.sources {
        p.url(file, "source", &s.url, Some(&allowed));
    }
    for m in &f.manuals {
        if !MANUAL_KINDS.contains(&m.kind.as_str()) {
            p.add(file, format!("manual kind '{}' is not known", m.kind));
        }
        p.text(file, "manual title", &m.title, 120);
    }
    for fa in &f.facts {
        if fa.topic.is_empty()
            || fa.topic.len() > 32
            || fa
                .topic
                .chars()
                .any(|c| !(c.is_ascii_lowercase() || c == ' ' || c == '-' || c == '_'))
        {
            p.add(
                file,
                format!("fact topic '{}' is not a short word", fa.topic),
            );
        }
        p.text(file, &format!("fact '{}'", fa.topic), &fa.text, 300);
    }
    let parent_tasks: BTreeSet<&str> = parent
        .as_ref()
        .map(|t| t.tasks.iter().map(|t| t.key.as_str()).collect())
        .unwrap_or_default();
    let own_tasks: BTreeSet<&str> = f.tasks.iter().map(|t| t.key.as_str()).collect();
    for c in &f.consumables {
        p.text(file, "consumable name", &c.name, 80);
        if c.part.is_none() && c.size.is_none() && c.replace_every.is_none() {
            p.add(
                file,
                format!(
                    "consumable '{}' needs a part, size or replace_every",
                    c.name
                ),
            );
        }
        if let Some(r) = &c.replace_every
            && r.parse::<Interval>().is_err()
        {
            p.add(
                file,
                format!(
                    "consumable '{}': replace_every '{r}' is not an interval",
                    c.name
                ),
            );
        }
        if let Some(t) = &c.for_task
            && !parent_tasks.contains(t.as_str())
            && !own_tasks.contains(t.as_str())
        {
            p.add(
                file,
                format!("consumable '{}': for_task '{t}' is not a task", c.name),
            );
        }
    }
    let mut seen = BTreeSet::new();
    for t in &f.tasks {
        let what = format!("task '{}'", t.key);
        if !seen.insert(t.key.as_str()) {
            p.add(file, format!("{what} appears twice"));
        }
        if let Some(s) = &t.schedule {
            if let Err(e) = s.validate() {
                p.add(file, format!("{what}: {e}"));
            }
            if matches!(
                s,
                crate::schedule::Schedule::OneOff { .. }
                    | crate::schedule::Schedule::Fixed {
                        anchor: Some(_),
                        ..
                    }
            ) {
                p.add(file, format!("{what}: no one-off or anchored schedules"));
            }
        }
        if let Some(m) = t.minutes
            && !(1..=600).contains(&m)
        {
            p.add(file, format!("{what}: minutes must be 1 to 600"));
        }
        if let Some(st) = &t.steps {
            if !(MIN_STEPS..=MAX_STEPS).contains(&st.len()) {
                p.add(
                    file,
                    format!("{what}: needs {MIN_STEPS} to {MAX_STEPS} steps"),
                );
            }
            for s in st {
                p.text(file, &format!("{what} step"), s, 200);
            }
        }
        if let Some(w) = &t.why {
            p.text(file, &format!("{what} why"), w, MAX_WHY_CHARS);
        }
        if let Some(ti) = &t.title {
            p.text(file, &format!("{what} title"), ti, 100);
        }
        let is_override = parent_tasks.contains(t.key.as_str());
        if !is_override
            && (t.title.is_none()
                || t.priority.is_none()
                || t.schedule.is_none()
                || t.minutes.is_none()
                || t.steps.is_none()
                || t.why.is_none())
        {
            p.add(
                file,
                format!(
                    "{what} is not a task of '{}', so it must be a whole task",
                    extends.as_deref().unwrap_or("its template")
                ),
            );
        }
    }
    if let Some(w) = &f.warranty {
        p.text(file, "warranty summary", &w.summary, 300);
    }
    for r in &f.recalls {
        p.text(file, "recall title", &r.title, 200);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_urls() {
        assert_eq!(
            https_host("https://www.Rheem.com/x?y=1").unwrap(),
            "www.rheem.com"
        );
        assert!(https_host("http://rheem.com/").is_err());
        assert!(https_host("https://user@rheem.com/").is_err());
        assert!(https_host("https://rheem.com:8443/").is_err());
        assert!(https_host("https://rheem.com/a b").is_err());
        assert!(https_host("https://rheem.com/?utm_source=x").is_err());
        let d = vec!["rheem.com".to_string()];
        assert!(host_allowed("www.rheem.com", &d));
        assert!(host_allowed("docs.rheem.com", &d));
        assert!(!host_allowed("notrheem.com", &d));
        assert!(!host_allowed("rheem.com.evil.net", &d));
        assert!(url_problem("https://evil.net/rheem.com", Some(&d)).is_some());
    }

    #[test]
    fn ids() {
        assert!(valid_id("ao-smith"));
        assert!(valid_id("3m"));
        assert!(!valid_id("Ao"));
        assert!(!valid_id("a--b"));
    }
}
