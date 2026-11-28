//! The appliance library (D-127 to D-129): brands, model families, links to
//! the manufacturers' official manual pages, and maintenance facts in our
//! own words, from the public repository `holdTheDoorHoid/housewright-library`
//! (CC BY 4.0). The app ships a pinned snapshot of it (`library/`) and may
//! download a newer published version (see [`bundle`]).
//!
//! Pure: text in, values out. Matching a make and model to a family is in
//! [`matching`]; it only ever suggests, and the person confirms.

pub mod bundle;
pub mod check;
pub mod matching;
pub mod model;

use std::collections::BTreeMap;

use crate::template::Library as Templates;

pub use bundle::{Bundle, Manifest};
pub use check::Problems;
pub use model::*;

/// A loaded, checked library.
#[derive(Clone, Debug)]
pub struct Library {
    pub meta: SnapshotMeta,
    pub shared_domains: Vec<String>,
    pub categories: BTreeMap<String, Category>,
    pub brands: BTreeMap<String, Brand>,
    /// Keyed by the full id, `<brand>/<family>`.
    pub families: BTreeMap<String, Family>,
    /// Files that failed a check and were left out (shipped snapshot only;
    /// a downloaded update with any problem is refused whole).
    pub skipped: Vec<String>,
    /// Every problem found (shipped snapshot only), one line each.
    pub problems: Vec<String>,
}

/// Uppercase letters and digits only: how brand names are compared
/// ("A. O. Smith" and "AO SMITH" are both `AOSMITH`).
pub fn normalise_brand(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A model number or pattern without case, spaces, hyphens, slashes, dots
/// or underscores (the library's FORMAT.md).
pub fn normalise_model(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '/' | '.' | '_'))
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Why a model pattern is unusable, if it is.
pub fn pattern_problem(p: &str) -> Option<String> {
    let n = normalise_model(p);
    if n.is_empty() {
        return Some("is empty".into());
    }
    if let Some(c) = n
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '#' | '@' | '?' | '*')))
    {
        return Some(format!("contains '{c}'"));
    }
    if n.chars().take_while(char::is_ascii_alphanumeric).count() < 2 {
        return Some("must start with at least two literal letters or digits".into());
    }
    None
}

impl Library {
    /// Load a bundle. `strict`: any problem refuses the whole bundle (for
    /// downloaded updates). Otherwise files with problems are left out and
    /// listed in [`Library::skipped`] (the shipped snapshot, which a test
    /// keeps free of problems).
    pub fn load(b: &Bundle, templates: &Templates, strict: bool) -> Result<Self, String> {
        let mut problems = Problems::default();
        let mut skipped = Vec::new();
        let meta: SnapshotMeta = parse(b, "SNAPSHOT.toml")?;
        if meta.format != 1 {
            return Err(format!(
                "library snapshot format {} is not supported",
                meta.format
            ));
        }
        if bundle::parse_version(&meta.version).is_none() {
            return Err(format!("'{}' is not a library version", meta.version));
        }
        let domains: Domains = parse(b, "reference/domains.toml")?;
        let shared = domains.shared;

        let mut categories = BTreeMap::new();
        let mut brands = BTreeMap::new();
        let mut families = BTreeMap::new();
        for (path, text) in &b.files {
            let parts: Vec<&str> = path.split('/').collect();
            let stem = |f: &str| f.trim_end_matches(".toml").to_string();
            match parts.as_slice() {
                ["categories", f] => match toml::from_str::<Category>(text) {
                    Ok(c) if c.id == stem(f) => {
                        categories.insert(c.id.clone(), c);
                    }
                    Ok(_) => problems
                        .0
                        .push(format!("{path}: id must equal the file name")),
                    Err(e) => problems.0.push(format!("{path}: {}", e.message())),
                },
                ["brands", f] => match toml::from_str::<Brand>(text) {
                    Ok(x) if x.id == stem(f) => {
                        brands.insert(x.id.clone(), x);
                    }
                    Ok(_) => problems
                        .0
                        .push(format!("{path}: id must equal the file name")),
                    Err(e) => problems.0.push(format!("{path}: {}", e.message())),
                },
                ["families", bdir, f] => match toml::from_str::<Family>(text) {
                    Ok(x) if x.id == stem(f) && x.brand == *bdir => {
                        families.insert(format!("{bdir}/{}", x.id), x);
                    }
                    Ok(_) => problems.0.push(format!(
                        "{path}: id and brand must equal the file and folder names"
                    )),
                    Err(e) => problems.0.push(format!("{path}: {}", e.message())),
                },
                _ => {}
            }
        }

        for (id, c) in &categories {
            let mut p = Problems::default();
            check::check_category(&mut p, &format!("categories/{id}.toml"), c, templates);
            problems.0.extend(p.0);
        }
        let mut bad_brands = Vec::new();
        for (id, x) in &brands {
            let mut p = Problems::default();
            check::check_brand(&mut p, &format!("brands/{id}.toml"), x, &shared);
            if !p.is_empty() {
                bad_brands.push(id.clone());
                problems.0.extend(p.0);
            }
        }
        for id in &bad_brands {
            brands.remove(id);
            skipped.push(format!("brands/{id}.toml"));
        }
        let mut bad_families = Vec::new();
        for (id, f) in &families {
            let file = format!("families/{id}.toml");
            let mut p = Problems::default();
            match (brands.get(&f.brand), categories.get(&f.category)) {
                (Some(brand), Some(cat)) => {
                    // A family may have no template yet (thermostats,
                    // ranges): it still has links and facts.
                    check::check_family(&mut p, &file, f, brand, cat, &shared, templates);
                }
                (None, _) => p.0.push(format!("{file}: brand '{}' is missing", f.brand)),
                (_, None) => {
                    p.0.push(format!("{file}: category '{}' is missing", f.category))
                }
            }
            if !p.is_empty() {
                bad_families.push(id.clone());
                problems.0.extend(p.0);
            }
        }
        for id in &bad_families {
            families.remove(id);
            skipped.push(format!("families/{id}.toml"));
        }
        if strict && !problems.is_empty() {
            let n = problems.0.len();
            let first = problems
                .0
                .into_iter()
                .take(3)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!("{n} problems in the library ({first})"));
        }
        Ok(Self {
            problems: problems.0,
            meta,
            shared_domains: shared,
            categories,
            brands,
            families,
            skipped,
        })
    }

    /// The generic template a family builds on.
    pub fn template_of(&self, f: &Family) -> Option<String> {
        f.extends.clone().or_else(|| {
            self.categories
                .get(&f.category)
                .and_then(|c| c.template.clone())
        })
    }

    /// Whether the family belongs to a life-safety category (alarms, gas).
    pub fn life_safety(&self, f: &Family) -> bool {
        self.categories
            .get(&f.category)
            .is_some_and(|c| c.life_safety)
    }

    pub fn family(&self, id: &str) -> Option<&Family> {
        self.families.get(id)
    }

    /// The domains a family's links may be on: its brand's own (manual
    /// downloads use only these, never the shared government domains).
    pub fn brand_domains(&self, f: &Family) -> Vec<String> {
        self.brands
            .get(&f.brand)
            .map(|b| b.domains.clone())
            .unwrap_or_default()
    }
}

fn parse<T: serde::de::DeserializeOwned>(b: &Bundle, path: &str) -> Result<T, String> {
    let text = b
        .files
        .get(path)
        .ok_or_else(|| format!("the library has no {path}"))?;
    toml::from_str(text).map_err(|e| format!("{path}: {}", e.message()))
}
