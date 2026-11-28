//! Maintenance templates (D-21): the TOML file format, loading, validation,
//! inheritance (model -> generic -> category) and turning a template into
//! tasks for one item. The format is documented in `templates/README.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fields::{FieldDef, FieldType, FieldValue, is_key};
use crate::ids::{ItemId, MemberId, MeterId, TaskId};
use crate::model::{Entity, EntityKind, Item, Meter, Task, TemplateTaskRef, Why};
use crate::priority::PriorityClass;
use crate::schedule::{Schedule, WindowPolicy};

/// The only template format version this build understands.
pub const TEMPLATE_FORMAT: u32 = 1;

/// Steps per task (P2: "3 to 7 short steps").
pub const MIN_STEPS: usize = 3;
pub const MAX_STEPS: usize = 7;
/// "Why it matters" is one line.
pub const MAX_WHY_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    /// A category or a generic item kind ("washing machine").
    Generic,
    /// A specific make and model; must extend a generic template.
    Model,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub format: u32,
    /// Lowercase letters, digits and hyphens; equals the file name.
    pub id: String,
    pub name: String,
    pub kind: TemplateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Parent template id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Fields recorded on the item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldDef>,
    /// Usage counters an item of this kind has.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meters: Vec<MeterDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<TemplateTask>,
}

impl Entity for Template {
    const KIND: EntityKind = EntityKind::Template;
    fn entity_id(&self) -> String {
        self.id.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeterDef {
    pub key: String,
    pub name: String,
    pub unit: String,
}

/// A recommended task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateTask {
    pub key: String,
    pub title: String,
    pub priority: PriorityClass,
    pub schedule: Schedule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPolicy>,
    pub steps: Vec<String>,
    /// One line: why this matters.
    pub why: String,
    /// The kind of source the recommendation comes from ("NFPA guidance").
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completion_fields: Vec<FieldDef>,
    /// Only for items whose field has this value (e.g. fuel = gas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: String,
    /// For a choice field, one of its options; for a yes/no field, "yes" or "no".
    pub equals: String,
}

impl Condition {
    pub fn holds(&self, item: &Item) -> bool {
        match item.custom.get(&self.field).map(|c| &c.value) {
            Some(FieldValue::Choice(v)) => v == &self.equals,
            Some(FieldValue::Text(v)) => v == &self.equals,
            Some(FieldValue::YesNo(b)) => (self.equals == "yes") == *b,
            _ => false,
        }
    }
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn check_unique<'a>(what: &str, keys: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for k in keys {
        if !seen.insert(k) {
            return Err(format!("{what} key '{k}' appears twice"));
        }
    }
    Ok(())
}

impl Template {
    /// Parse one TOML file. Does not validate.
    pub fn from_toml(text: &str, file: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| Error::Template {
            file: file.to_string(),
            message: e.to_string(),
        })
    }

    /// Checks that need only this file.
    pub fn validate(&self) -> Result<(), String> {
        if self.format != TEMPLATE_FORMAT {
            return Err(format!(
                "format {} is not supported (expected {TEMPLATE_FORMAT})",
                self.format
            ));
        }
        if !is_slug(&self.id) {
            return Err(format!(
                "id '{}' must be lowercase letters, digits and hyphens",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err("name is empty".into());
        }
        if self.kind == TemplateKind::Model
            && (self.extends.is_none() || self.make.is_none() || self.model.is_none())
        {
            return Err("a model template needs extends, make and model".into());
        }
        if self.extends.as_deref() == Some(self.id.as_str()) {
            return Err("a template cannot extend itself".into());
        }
        check_unique("field", self.fields.iter().map(|f| f.key.as_str()))?;
        for f in &self.fields {
            f.validate()?;
        }
        check_unique("meter", self.meters.iter().map(|m| m.key.as_str()))?;
        for m in &self.meters {
            if !is_key(&m.key) || m.name.trim().is_empty() || m.unit.trim().is_empty() {
                return Err(format!("meter '{}' needs a key, name and unit", m.key));
            }
        }
        check_unique("task", self.tasks.iter().map(|t| t.key.as_str()))?;
        for t in &self.tasks {
            t.validate().map_err(|e| format!("task '{}': {e}", t.key))?;
        }
        Ok(())
    }

    /// Checks that need the fully resolved template (after inheritance).
    fn validate_resolved(&self) -> Result<(), String> {
        let meters: BTreeSet<&str> = self.meters.iter().map(|m| m.key.as_str()).collect();
        for t in &self.tasks {
            if let Schedule::Usage { meter, .. } = &t.schedule
                && !meters.contains(meter.as_str())
            {
                return Err(format!("task '{}': unknown meter '{meter}'", t.key));
            }
            if let Some(c) = &t.when {
                let Some(f) = self.fields.iter().find(|f| f.key == c.field) else {
                    return Err(format!("task '{}': unknown field '{}'", t.key, c.field));
                };
                let ok = match &f.field_type {
                    FieldType::Choice { options } => options.contains(&c.equals),
                    FieldType::YesNo => c.equals == "yes" || c.equals == "no",
                    FieldType::Text => true,
                    _ => false,
                };
                if !ok {
                    return Err(format!(
                        "task '{}': condition {}={} cannot match that field",
                        t.key, c.field, c.equals
                    ));
                }
            }
        }
        Ok(())
    }

    /// The tasks and meters this template gives one item. Tasks start
    /// inactive: they are the suggested plan, switched on a few at a time (P3).
    pub fn plan_for_item(
        &self,
        item: &Item,
        owner: MemberId,
        start: Date,
    ) -> (Vec<Meter>, Vec<Task>) {
        let meters = self
            .meters
            .iter()
            .map(|m| Meter {
                id: MeterId::new(),
                item: Some(item.id),
                key: m.key.clone(),
                name: m.name.clone(),
                unit: m.unit.clone(),
            })
            .collect();
        let tasks = self
            .tasks
            .iter()
            .filter(|t| t.when.as_ref().is_none_or(|c| c.holds(item)))
            .map(|t| t.to_task(&self.id, item.property, Some(item.id), owner, start))
            .collect();
        (meters, tasks)
    }
}

impl TemplateTask {
    fn validate(&self) -> Result<(), String> {
        if !is_key(&self.key) {
            return Err("key must be lowercase letters, digits and underscores".into());
        }
        if self.title.trim().is_empty() {
            return Err("title is empty".into());
        }
        if !(MIN_STEPS..=MAX_STEPS).contains(&self.steps.len()) {
            return Err(format!(
                "has {} steps; give {MIN_STEPS} to {MAX_STEPS}",
                self.steps.len()
            ));
        }
        if self.steps.iter().any(|s| s.trim().is_empty()) {
            return Err("a step is empty".into());
        }
        let why = self.why.trim();
        if why.is_empty() || why.contains('\n') || why.chars().count() > MAX_WHY_CHARS {
            return Err(format!(
                "why must be one line of at most {MAX_WHY_CHARS} characters"
            ));
        }
        if self.source.trim().is_empty() {
            return Err("source is empty".into());
        }
        self.schedule.validate()?;
        match &self.schedule {
            Schedule::OneOff { .. } => {
                return Err("templates cannot contain one-off tasks".into());
            }
            Schedule::Fixed {
                anchor: Some(_), ..
            } => {
                return Err("templates cannot fix an anchor date".into());
            }
            _ => {}
        }
        check_unique(
            "completion field",
            self.completion_fields.iter().map(|f| f.key.as_str()),
        )?;
        for f in &self.completion_fields {
            f.validate()?;
        }
        Ok(())
    }

    /// Make a concrete task.
    pub fn to_task(
        &self,
        template_id: &str,
        property: crate::ids::PropertyId,
        item: Option<ItemId>,
        owner: MemberId,
        start: Date,
    ) -> Task {
        Task {
            id: TaskId::new(),
            property,
            item,
            room: None,
            title: self.title.clone(),
            owner,
            priority: self.priority,
            schedule: self.schedule.clone(),
            window: self.window,
            start,
            steps: self.steps.clone(),
            why: Some(Why {
                text: self.why.clone(),
                source: self.source.clone(),
            }),
            minutes: self.minutes,
            completion_fields: self.completion_fields.clone(),
            template: Some(TemplateTaskRef {
                template: template_id.to_string(),
                task: self.key.clone(),
            }),
            active: false,
            paused: false,
            not_applicable: false,
        }
    }
}

/// A validated set of templates.
#[derive(Clone, Debug, Default)]
pub struct Library {
    templates: BTreeMap<String, Template>,
}

impl Library {
    /// Build from parsed templates, validating each one, every parent link and
    /// every resolved template.
    pub fn new(templates: Vec<(String, Template)>) -> Result<Self> {
        let mut map = BTreeMap::new();
        for (file, t) in templates {
            t.validate().map_err(|message| Error::Template {
                file: file.clone(),
                message,
            })?;
            if map.contains_key(&t.id) {
                return Err(Error::Template {
                    file,
                    message: format!("duplicate template id '{}'", t.id),
                });
            }
            map.insert(t.id.clone(), t);
        }
        let lib = Library { templates: map };
        for id in lib.templates.keys() {
            let resolved = lib.resolve(id)?;
            resolved
                .validate_resolved()
                .map_err(|message| Error::Template {
                    file: format!("{id}.toml"),
                    message,
                })?;
        }
        Ok(lib)
    }

    /// Load every `*.toml` file under `dir` (recursively). Each file's name
    /// must be its template id.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let mut files = Vec::new();
        collect_toml(dir.as_ref(), &mut files)?;
        files.sort();
        let mut parsed = Vec::new();
        for path in files {
            let name = path.display().to_string();
            let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
                path: name.clone(),
                source,
            })?;
            let t = Template::from_toml(&text, &name)?;
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem != t.id {
                return Err(Error::Template {
                    file: name,
                    message: format!("file name must be '{}.toml'", t.id),
                });
            }
            parsed.push((name, t));
        }
        Self::new(parsed)
    }

    pub fn get(&self, id: &str) -> Option<&Template> {
        self.templates.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.templates.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// The template with everything inherited from its parents merged in.
    /// Children override fields, meters and tasks with the same key.
    pub fn resolve(&self, id: &str) -> Result<Template> {
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cur = Some(id.to_string());
        while let Some(c) = cur {
            if !seen.insert(c.clone()) {
                return Err(Error::Template {
                    file: format!("{id}.toml"),
                    message: format!("inheritance cycle through '{c}'"),
                });
            }
            let t = self.templates.get(&c).ok_or_else(|| Error::Template {
                file: format!("{id}.toml"),
                message: format!("unknown template '{c}'"),
            })?;
            chain.push(t);
            cur = t.extends.clone();
        }
        let leaf = chain[0];
        let mut out = leaf.clone();
        out.extends = None;
        out.fields = merge_by_key(chain.iter().rev().map(|t| &t.fields), |f| &f.key);
        out.meters = merge_by_key(chain.iter().rev().map(|t| &t.meters), |m| &m.key);
        out.tasks = merge_by_key(chain.iter().rev().map(|t| &t.tasks), |t| &t.key);
        out.category = chain.iter().find_map(|t| t.category.clone());
        out.description = chain.iter().find_map(|t| t.description.clone());
        Ok(out)
    }
}

/// Merge lists root-first; a later list replaces entries with the same key in
/// place and appends new ones.
fn merge_by_key<'a, T: Clone + 'a>(
    lists: impl Iterator<Item = &'a Vec<T>>,
    key: impl Fn(&T) -> &String,
) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for list in lists {
        for item in list {
            match out.iter().position(|o| key(o) == key(item)) {
                Some(i) => out[i] = item.clone(),
                None => out.push(item.clone()),
            }
        }
    }
    out
}

fn collect_toml(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.display().to_string(),
        source,
    })?;
    for e in entries {
        let e = e.map_err(|source| Error::Io {
            path: dir.display().to_string(),
            source,
        })?;
        let p = e.path();
        if p.is_dir() {
            collect_toml(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "toml") {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::CustomValue;
    use crate::ids::PropertyId;
    use crate::model::Sensitivity;
    use jiff::civil::date;

    const GENERIC: &str = r#"
format = 1
id = "water-heater"
name = "Water heater"
kind = "generic"
category = "plumbing"

[[fields]]
key = "fuel"
label = "Fuel"
type = "choice"
options = ["gas", "electric"]

[[fields]]
key = "capacity"
label = "Capacity"
type = "number"
unit = "gal"

[[tasks]]
key = "flush"
title = "Flush the tank"
priority = "efficiency"
schedule = { kind = "fixed", every = "1 year" }
steps = ["a", "b", "c"]
why = "Sediment wastes energy."
source = "common manufacturer guidance"

[[tasks]]
key = "vent"
title = "Look at the flue"
priority = "life_safety"
schedule = { kind = "floating", every = "6 months" }
steps = ["a", "b", "c"]
why = "A blocked flue can leak exhaust."
source = "common manufacturer guidance"
when = { field = "fuel", equals = "gas" }
"#;

    const MODEL: &str = r#"
format = 1
id = "acme-wh50"
name = "Acme WH50"
kind = "model"
extends = "water-heater"
make = "Acme"
model = "WH50"

[[tasks]]
key = "flush"
title = "Flush the tank (Acme)"
priority = "efficiency"
schedule = { kind = "fixed", every = "6 months" }
steps = ["a", "b", "c", "d"]
why = "Sediment wastes energy."
source = "template author"
"#;

    fn lib() -> Library {
        Library::new(vec![
            ("g".into(), Template::from_toml(GENERIC, "g").unwrap()),
            ("m".into(), Template::from_toml(MODEL, "m").unwrap()),
        ])
        .unwrap()
    }

    #[test]
    fn inheritance_overrides_by_key() {
        let r = lib().resolve("acme-wh50").unwrap();
        assert_eq!(r.fields.len(), 2);
        assert_eq!(r.tasks.len(), 2);
        assert_eq!(r.tasks[0].title, "Flush the tank (Acme)");
        assert_eq!(r.category.as_deref(), Some("plumbing"));
        assert_eq!(r.extends, None);
    }

    #[test]
    fn conditions_filter_tasks() {
        let r = lib().resolve("water-heater").unwrap();
        let mut item = Item {
            id: ItemId::new(),
            property: PropertyId::new(),
            room: None,
            parent: None,
            name: "WH".into(),
            category: None,
            make: None,
            model: None,
            serial: None,
            purchased: None,
            installed: None,
            warranty: None,
            location_note: None,
            template: Some("water-heater".into()),
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Sensitivity::Normal,
            price: Default::default(),
            value: Default::default(),
        };
        let owner = MemberId::new();
        let (_, tasks) = r.plan_for_item(&item, owner, date(2026, 1, 1));
        assert_eq!(tasks.len(), 1);
        item.custom.insert(
            "fuel".into(),
            CustomValue {
                value: FieldValue::Choice("gas".into()),
                sensitivity: Sensitivity::Normal,
            },
        );
        let (_, tasks) = r.plan_for_item(&item, owner, date(2026, 1, 1));
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| !t.active && t.owner == owner));
    }

    fn expect_err(text: &str, needle: &str) {
        let t = Template::from_toml(text, "x");
        let err = match t {
            Err(e) => e.to_string(),
            Ok(t) => match Library::new(vec![("x".into(), t)]) {
                Err(e) => e.to_string(),
                Ok(_) => panic!("expected an error containing '{needle}'"),
            },
        };
        assert!(err.contains(needle), "{err}");
    }

    #[test]
    fn validation_errors() {
        expect_err(&GENERIC.replace("format = 1", "format = 2"), "format 2");
        expect_err(
            &GENERIC.replace(r#"["a", "b", "c"]"#, r#"["a", "b"]"#),
            "2 steps",
        );
        expect_err(
            &GENERIC.replace("Sediment wastes energy.", ""),
            "why must be one line",
        );
        expect_err(
            &GENERIC.replace(r#"equals = "gas""#, r#"equals = "oil""#),
            "cannot match",
        );
        expect_err(
            &GENERIC.replace(
                r#"{ kind = "floating", every = "6 months" }"#,
                r#"{ kind = "usage", meter = "hours", threshold = 10.0 }"#,
            ),
            "unknown meter",
        );
        expect_err(
            &GENERIC.replace("id = \"water-heater\"", "id = \"Water\""),
            "id",
        );
        expect_err(
            &GENERIC.replace("kind = \"generic\"", "kind = \"model\""),
            "model template",
        );
        expect_err(&format!("{GENERIC}\nsurprise = 1\n"), "surprise");
        expect_err(
            &GENERIC.replace(r#"every = "1 year""#, r#"every = "1 fortnight""#),
            "fortnight",
        );
    }

    #[test]
    fn unknown_parent_and_cycles() {
        let orphan = Template::from_toml(MODEL, "m").unwrap();
        assert!(Library::new(vec![("m".into(), orphan)]).is_err());

        let mut a = Template::from_toml(GENERIC, "a").unwrap();
        a.id = "a".into();
        a.extends = Some("b".into());
        let mut b = a.clone();
        b.id = "b".into();
        b.extends = Some("a".into());
        let err = Library::new(vec![("a".into(), a), ("b".into(), b)])
            .unwrap_err()
            .to_string();
        assert!(err.contains("cycle"), "{err}");
    }
}
