//! The appliance library in the app (D-147 to D-152): matching a make and
//! model to a model family, the "From the manufacturer" part of a thing's
//! page, the manufacturer-or-generic choice per task, optional library
//! updates, and saving the official manual onto the device.
//!
//! Nothing here touches the network. Library updates are downloaded by the
//! interface (a plain GET of two public files) and handed to
//! `library_check` / `library_import`; manuals are downloaded by the native
//! shell through [`download_manual`] and handed to `attach_manual`.

use housewright_core::appliances::{
    self, Bundle, Family, Library, Manifest, SnapshotMeta, TaskOverride, bundle, matching,
};
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::priority::PriorityClass;
use housewright_core::records::NewFile;
use housewright_core::template::TemplateTask;
use housewright_media::photo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

/// Where published library versions are read from: the library
/// repository's `published` branch, served by GitHub's raw file host (which
/// allows any origin, so the web build can read it too). Two files:
/// `latest.json` and the bundle it names.
pub const UPDATE_BASE: &str =
    "https://raw.githubusercontent.com/holdTheDoorHoid/housewright-library/published/";

/// A manual is downloaded only up to this size (the largest document
/// Housewright keeps).
pub const MAX_MANUAL_BYTES: usize = housewright_core::records::MAX_DOCUMENT_BYTES;
/// Redirects followed when downloading a manual, each checked.
pub const MAX_MANUAL_REDIRECTS: usize = 5;

const META_UPDATE: &str = "library_update";
const META_AUTO: &str = "library_auto";
const META_CHECKED: &str = "library_checked_at";

/// The library in use: the shipped snapshot, or a newer downloaded one.
pub(crate) struct Appliances {
    pub lib: Library,
    pub shipped: SnapshotMeta,
    pub from_update: bool,
}

/// The snapshot compiled into the app.
pub fn shipped_library(templates: &housewright_core::template::Library) -> Result<Library, String> {
    let b = Bundle::from_gz(crate::templates::LIBRARY_GZ)?;
    Library::load(&b, templates, false)
}

// -- output -----------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize)]
pub struct MatchOut {
    pub brands: Vec<BrandRef>,
    pub families: Vec<FamilyHit>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BrandRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FamilyHit {
    pub id: String,
    pub name: String,
    pub brand_name: String,
    /// The generic template it builds on, when the app has one.
    pub template: Option<TemplateRef>,
    pub confidence: matching::Confidence,
    pub reasons: Vec<String>,
    pub pattern: Option<String>,
    pub corrected_model: Option<String>,
    pub unverified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Link {
    pub title: String,
    pub url: String,
    /// When the library last checked the page (`2026-11-28`).
    pub checked: Option<String>,
    pub unverified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManualOut {
    /// manual-search | product-page | support-page | manual-pdf | parts-page
    pub kind: String,
    pub title: String,
    pub url: String,
    /// A direct link to a PDF, which the app can save onto the device.
    pub pdf: bool,
    pub unverified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MakerFact {
    pub topic: String,
    pub text: String,
    pub source: String,
    pub checked: Option<String>,
    pub unverified: bool,
}

/// A filter, anode or other part, as structured data (for the coming
/// "tools and supplies" list).
#[derive(Clone, Debug, Serialize)]
pub struct ConsumableOut {
    pub name: String,
    pub part: Option<String>,
    pub alternates: Vec<String>,
    pub size: Option<String>,
    pub replace_every: Option<String>,
    pub for_task: Option<String>,
    /// The thing's task it is used in, when there is one.
    pub task_title: Option<String>,
    pub source: String,
    pub checked: Option<String>,
    pub unverified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct WarrantySummary {
    pub summary: String,
    pub length: Option<String>,
    pub source: String,
    pub checked: Option<String>,
    pub unverified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecallOut {
    pub title: String,
    pub date: Option<String>,
    pub url: String,
}

/// One task where the manufacturer's advice differs from the generic one.
#[derive(Clone, Debug, Serialize)]
pub struct GuidanceOut {
    pub task_id: String,
    pub task_title: String,
    pub key: String,
    /// "Every 3 months"
    pub generic: String,
    pub generic_source: String,
    pub maker: String,
    /// The maker's own title and steps, when they differ.
    pub maker_title: Option<String>,
    pub maker_steps: Option<Vec<String>>,
    pub note: Option<String>,
    pub source: String,
    pub checked: Option<String>,
    pub unverified: bool,
    /// `maker`, `generic`, or absent while the person has not chosen.
    pub following: Option<Follows>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FamilyOut {
    pub id: String,
    pub name: String,
    pub brand_id: String,
    pub brand_name: String,
    pub category: String,
    pub template: Option<TemplateRef>,
    pub status: String,
    pub years: Option<String>,
    pub description: Option<String>,
    pub model_patterns: Vec<String>,
    pub model_examples: Vec<String>,
    pub unverified: bool,
    pub note: Option<String>,
    pub website: String,
    pub support_url: String,
    pub manual_search_url: Option<String>,
    pub phone: Option<String>,
    pub manuals: Vec<ManualOut>,
    pub facts: Vec<MakerFact>,
    pub consumables: Vec<ConsumableOut>,
    pub warranty: Option<WarrantySummary>,
    pub recalls: Vec<RecallOut>,
    /// The credit CC BY 4.0 asks for, with the library version.
    pub credit: String,
}

/// The "From <Brand>" part of a thing's page.
#[derive(Clone, Debug, Serialize)]
pub struct MakerOut {
    pub family_id: String,
    /// False when the library in use no longer lists the family.
    pub in_library: bool,
    pub family: Option<FamilyOut>,
    pub guidance: Vec<GuidanceOut>,
    /// The thing's kind differs from the family's (the person chose it
    /// anyway); task advice is not offered then.
    pub other_kind: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LibraryStatus {
    pub version: String,
    pub commit: String,
    pub published: String,
    pub source: String,
    pub licence: String,
    pub credit: String,
    pub brands: usize,
    pub families: usize,
    /// True when a downloaded newer version is in use.
    pub from_update: bool,
    pub shipped_version: String,
    /// Check for updates by itself (at most weekly, when the app opens).
    pub auto: bool,
    pub checked_at: Option<String>,
    /// The one file a check reads first.
    pub manifest_url: String,
    /// Files in the shipped snapshot that failed a check (should be 0).
    pub problems: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateCheck {
    pub newer: bool,
    pub version: String,
    pub size: u64,
    /// Where to get the bundle, when it is newer.
    pub bundle_url: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManualSource {
    pub url: String,
    /// The file name it will be saved under.
    pub name: String,
    /// The only hosts the download (and every redirect) may use.
    pub domains: Vec<String>,
}

/// A thing's parts across the house (for "tools and supplies").
#[derive(Clone, Debug, Serialize)]
pub struct SupplyOut {
    pub item_id: String,
    pub item_name: String,
    pub family_id: String,
    pub consumable: ConsumableOut,
}

// -- arguments --------------------------------------------------------------

#[derive(Deserialize)]
struct MatchArgs {
    #[serde(default)]
    make: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    template: Option<String>,
}

#[derive(Deserialize)]
struct FamiliesArgs {
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    brand: Option<String>,
}

#[derive(Deserialize)]
struct SetFamilyArgs {
    item_id: String,
    /// A family id, `"none"`, or null to clear.
    #[serde(default)]
    family: Option<String>,
}

#[derive(Deserialize)]
struct GuidanceArgs {
    task_id: String,
    follow: Follows,
}

#[derive(Deserialize)]
struct AutoArgs {
    auto: bool,
}

#[derive(Deserialize)]
struct ManifestArgs {
    manifest: String,
}

#[derive(Deserialize)]
pub(crate) struct ManualArgs {
    item_id: String,
    url: String,
}

// -- helpers ------------------------------------------------------------------

fn not_found(what: &str) -> ApiError {
    ApiError {
        code: "not_found".into(),
        message: what.into(),
    }
}

/// "Housewright appliance library contributors, CC BY 4.0, version …".
fn credit(m: &SnapshotMeta) -> String {
    format!(
        "From the Housewright appliance library ({}), {}, version {}.",
        m.credit, m.licence, m.version
    )
}

fn manual_name(f: &Family, brand: &str, url: &str) -> String {
    let base = url
        .rsplit('/')
        .next()
        .unwrap_or("")
        .split(['?', '#'])
        .next()
        .unwrap_or("");
    let clean: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if clean.to_ascii_lowercase().ends_with(".pdf") && clean.len() > 4 {
        format!("{brand} manual - {clean}")
    } else {
        format!("{} manual.pdf", f.name)
    }
}

/// The generic task with the family's override applied (D-150).
pub(crate) fn merged(generic: &TemplateTask, ov: &TaskOverride, brand: &str) -> TemplateTask {
    let mut t = generic.clone();
    if let Some(x) = &ov.title {
        t.title = x.clone();
    }
    if let Some(x) = ov.priority {
        t.priority = x;
    }
    if let Some(x) = &ov.schedule {
        t.schedule = x.clone();
        // The generic window was set for the generic interval.
        t.window = None;
    }
    if let Some(x) = ov.window {
        t.window = Some(x);
    }
    if let Some(x) = &ov.steps {
        t.steps = x.clone();
    }
    if let Some(x) = ov.minutes {
        t.minutes = Some(x);
    }
    if let Some(x) = &ov.why {
        t.why = x.clone();
    }
    if let Some(x) = &ov.completion_fields {
        t.completion_fields = x.clone();
    }
    t.source = format!("manufacturer guidance ({brand})");
    t
}

impl Api {
    pub(crate) fn appliances(&self) -> &Appliances {
        self.appliances.get_or_init(|| {
            let shipped = shipped_library(&self.library).unwrap_or_else(|e| {
                // A test keeps the shipped snapshot loadable; never stop the
                // app over it.
                panic!("the shipped appliance library does not load: {e}")
            });
            let meta = shipped.meta.clone();
            let update = self
                .store
                .meta(META_UPDATE)
                .ok()
                .flatten()
                .and_then(|text| Bundle::from_json(text.as_bytes()).ok())
                .and_then(|b| Library::load(&b, &self.library, true).ok())
                .filter(|l| bundle::is_newer(&l.meta.version, &meta.version));
            match update {
                Some(lib) => Appliances {
                    lib,
                    shipped: meta,
                    from_update: true,
                },
                None => Appliances {
                    lib: shipped,
                    shipped: meta,
                    from_update: false,
                },
            }
        })
    }

    pub(crate) fn library_call(&mut self, cmd: &str, a: Value) -> Option<Result<Value, ApiError>> {
        Some(match cmd {
            "library" => ok(self.library_status()),
            "library_set_updates" => args::<AutoArgs>(a).and_then(|a| {
                self.store
                    .set_meta(META_AUTO, if a.auto { "1" } else { "0" })?;
                ok(self.library_status())
            }),
            "library_check" => {
                args::<ManifestArgs>(a).and_then(|a| ok(self.library_check(&a.manifest)?))
            }
            "library_reset" => (|| {
                self.store.set_meta(META_UPDATE, "")?;
                self.appliances = std::cell::OnceCell::new();
                ok(self.library_status())
            })(),
            "library_match" => args::<MatchArgs>(a)
                .and_then(|a| ok(self.match_out(&a.make, &a.model, a.template.as_deref()))),
            "library_families" => args::<FamiliesArgs>(a).and_then(|a| ok(self.families_list(a))),
            "library_family" => args::<IdArgs>(a).and_then(|a| {
                let app = self.appliances();
                let f = app
                    .lib
                    .family(&a.id)
                    .ok_or_else(|| not_found("the appliance library has no such family"))?;
                ok(self.family_out(&a.id, f))
            }),
            "set_family" => args::<SetFamilyArgs>(a).and_then(|a| ok(self.set_family(a)?)),
            "choose_guidance" => args::<GuidanceArgs>(a).and_then(|a| ok(self.choose_guidance(a)?)),
            "manual_source" => args::<ManualArgs>(a).and_then(|a| ok(self.manual_source(&a)?)),
            "supplies" => ok(self.supplies()),
            _ => return None,
        })
    }

    fn library_status(&self) -> LibraryStatus {
        let app = self.appliances();
        let m = &app.lib.meta;
        LibraryStatus {
            version: m.version.clone(),
            commit: m.commit.clone(),
            published: m.published.clone(),
            source: m.source.clone(),
            licence: m.licence.clone(),
            credit: credit(m),
            brands: app.lib.brands.len(),
            families: app.lib.families.len(),
            from_update: app.from_update,
            shipped_version: app.shipped.version.clone(),
            auto: self.store.meta(META_AUTO).ok().flatten().as_deref() == Some("1"),
            checked_at: self.store.meta(META_CHECKED).ok().flatten(),
            manifest_url: format!("{UPDATE_BASE}latest.json"),
            problems: app.lib.skipped.len(),
        }
    }

    fn library_check(&mut self, manifest: &str) -> Result<UpdateCheck, ApiError> {
        let m = Manifest::parse(manifest).map_err(ApiError::invalid)?;
        self.store
            .set_meta(META_CHECKED, &self.store.now().to_string())?;
        let current = self.appliances().lib.meta.version.clone();
        let newer = bundle::is_newer(&m.version, &current);
        Ok(UpdateCheck {
            newer,
            version: m.version.clone(),
            size: m.size,
            bundle_url: newer.then(|| format!("{UPDATE_BASE}{}", m.bundle)),
            message: if newer {
                format!(
                    "Version {} is available ({} KB).",
                    m.version,
                    m.size.div_ceil(1024)
                )
            } else {
                format!("You have the newest appliance library (version {current}).")
            },
        })
    }

    /// Check a downloaded bundle against its release note and every library
    /// rule, then use it. Anything wrong: nothing changes.
    pub(crate) fn library_import(&mut self, a: Value, bytes: &[u8]) -> Result<Value, ApiError> {
        let a: ManifestArgs = args(a)?;
        let m = Manifest::parse(&a.manifest).map_err(ApiError::invalid)?;
        m.verify(bytes).map_err(ApiError::invalid)?;
        let b = Bundle::from_gz(bytes).map_err(ApiError::invalid)?;
        let lib = Library::load(&b, &self.library, true).map_err(|e| {
            ApiError::invalid(format!(
                "the downloaded library failed its checks and was not used: {e}"
            ))
        })?;
        if lib.meta.version != m.version || lib.meta.commit != m.commit {
            return Err(ApiError::invalid(
                "the downloaded library is not the version its release note names; not used",
            ));
        }
        let current = self.appliances().lib.meta.version.clone();
        if !bundle::is_newer(&lib.meta.version, &current) {
            return Err(ApiError::invalid(format!(
                "version {} is not newer than the one in use ({current})",
                lib.meta.version
            )));
        }
        self.store.set_meta(META_UPDATE, &b.to_json())?;
        let shipped = self.appliances().shipped.clone();
        self.appliances = std::cell::OnceCell::new();
        let _ = self.appliances.set(Appliances {
            lib,
            shipped,
            from_update: true,
        });
        ok(self.library_status())
    }

    fn template_ref(&self, id: Option<&str>) -> Option<TemplateRef> {
        let t = self.library.get(id?)?;
        Some(TemplateRef {
            id: t.id.clone(),
            name: t.name.clone(),
        })
    }

    fn hit(&self, m: &matching::FamilyMatch) -> Option<FamilyHit> {
        let lib = &self.appliances().lib;
        let f = lib.family(&m.family)?;
        Some(FamilyHit {
            id: m.family.clone(),
            name: f.name.clone(),
            brand_name: lib
                .brands
                .get(&f.brand)
                .map(|b| b.name.clone())
                .unwrap_or_default(),
            template: self.template_ref(lib.template_of(f).as_deref()),
            confidence: m.confidence,
            reasons: m.reasons.clone(),
            pattern: m.pattern.clone(),
            corrected_model: m.corrected_model.clone(),
            unverified: f.unverified,
        })
    }

    pub(crate) fn match_out(&self, make: &str, model: &str, template: Option<&str>) -> MatchOut {
        let lib = &self.appliances().lib;
        let r = matching::find(lib, make, model, template.filter(|t| !t.is_empty()));
        MatchOut {
            brands: r
                .brands
                .iter()
                .filter_map(|b| {
                    lib.brands.get(b).map(|x| BrandRef {
                        id: b.clone(),
                        name: x.name.clone(),
                    })
                })
                .collect(),
            families: r.families.iter().filter_map(|m| self.hit(m)).collect(),
        }
    }

    fn families_list(&self, a: FamiliesArgs) -> Vec<FamilyHit> {
        let lib = &self.appliances().lib;
        lib.families
            .iter()
            .filter(|(_, f)| a.brand.as_deref().is_none_or(|b| b == f.brand))
            .filter(|(_, f)| {
                a.template
                    .as_deref()
                    .is_none_or(|t| lib.template_of(f).as_deref() == Some(t))
            })
            .filter_map(|(id, f)| {
                self.hit(&matching::FamilyMatch {
                    family: id.clone(),
                    confidence: matching::Confidence::Brand,
                    reasons: vec![],
                    pattern: None,
                    corrected_model: None,
                })
                .map(|mut h| {
                    h.unverified = f.unverified;
                    h
                })
            })
            .collect()
    }

    pub(crate) fn family_out(&self, id: &str, f: &Family) -> FamilyOut {
        let lib = &self.appliances().lib;
        let brand = lib.brands.get(&f.brand);
        let resolved = lib
            .template_of(f)
            .and_then(|t| self.library.resolve(&t).ok());
        FamilyOut {
            id: id.to_string(),
            name: f.name.clone(),
            brand_id: f.brand.clone(),
            brand_name: brand.map(|b| b.name.clone()).unwrap_or_default(),
            category: f.category.clone(),
            template: self.template_ref(lib.template_of(f).as_deref()),
            status: f.status.clone(),
            years: f.years.clone(),
            description: f.description.clone(),
            model_patterns: f.model_patterns.clone(),
            model_examples: f.model_examples.clone(),
            unverified: f.unverified,
            note: f.note.clone(),
            website: brand.map(|b| b.website.clone()).unwrap_or_default(),
            support_url: brand.map(|b| b.support_url.clone()).unwrap_or_default(),
            manual_search_url: brand.and_then(|b| b.manual_search_url.clone()),
            phone: brand.and_then(|b| b.phone.clone()),
            manuals: f
                .manuals
                .iter()
                .map(|m| ManualOut {
                    kind: m.kind.clone(),
                    title: m.title.clone(),
                    url: m.url.clone(),
                    pdf: m.kind == "manual-pdf",
                    unverified: m.unverified,
                })
                .collect(),
            facts: f
                .facts
                .iter()
                .map(|x| MakerFact {
                    topic: x.topic.clone(),
                    text: x.text.clone(),
                    source: x.source.clone(),
                    checked: f.checked(&x.source).map(String::from),
                    unverified: x.unverified,
                })
                .collect(),
            consumables: f
                .consumables
                .iter()
                .map(|c| ConsumableOut {
                    name: c.name.clone(),
                    part: c.part.clone(),
                    alternates: c.alternates.clone(),
                    size: c.size.clone(),
                    replace_every: c.replace_every.clone(),
                    for_task: c.for_task.clone(),
                    task_title: c.for_task.as_ref().and_then(|k| {
                        resolved
                            .as_ref()
                            .and_then(|r| r.tasks.iter().find(|t| &t.key == k))
                            .map(|t| t.title.clone())
                    }),
                    source: c.source.clone(),
                    checked: f.checked(&c.source).map(String::from),
                    unverified: c.unverified,
                })
                .collect(),
            warranty: f.warranty.as_ref().map(|w| WarrantySummary {
                summary: w.summary.clone(),
                length: w.length.clone(),
                source: w.source.clone(),
                checked: f.checked(&w.source).map(String::from),
                unverified: w.unverified,
            }),
            recalls: f
                .recalls
                .iter()
                .map(|r| RecallOut {
                    title: r.title.clone(),
                    date: r.date.clone(),
                    url: r.url.clone(),
                })
                .collect(),
            credit: credit(&lib.meta),
        }
    }

    /// The overrides this family offers for one item's tasks.
    fn guidance_for(&self, item: &Item, f: &Family, tasks: &[Task]) -> Vec<GuidanceOut> {
        let lib = &self.appliances().lib;
        let Some(tpl) = item.template.as_deref() else {
            return vec![];
        };
        if lib.template_of(f).as_deref() != Some(tpl) {
            return vec![];
        }
        let Ok(generic) = self.library.resolve(tpl) else {
            return vec![];
        };
        let brand = lib
            .brands
            .get(&f.brand)
            .map(|b| b.name.as_str())
            .unwrap_or("");
        let fam_id = format!("{}/{}", f.brand, f.id);
        let mut out = Vec::new();
        for ov in &f.tasks {
            let Some(g) = generic.tasks.iter().find(|t| t.key == ov.key) else {
                continue; // a whole new task: not offered yet (none exist).
            };
            // Never let an unconfirmed entry change a life-safety schedule.
            if ov.unverified && (g.priority == PriorityClass::LifeSafety || lib.life_safety(f)) {
                continue;
            }
            let m = merged(g, ov, brand);
            if m.validate().is_err() {
                continue;
            }
            for t in tasks.iter().filter(|t| {
                t.template
                    .as_ref()
                    .is_some_and(|r| r.template == tpl && r.task == ov.key)
            }) {
                out.push(GuidanceOut {
                    task_id: t.id.to_string(),
                    task_title: t.title.clone(),
                    key: ov.key.clone(),
                    generic: g.schedule.describe(),
                    generic_source: g.source.clone(),
                    maker: m.schedule.describe(),
                    maker_title: ov.title.clone(),
                    maker_steps: ov.steps.clone(),
                    note: ov.note.clone(),
                    source: ov.source.clone(),
                    checked: f.checked(&ov.source).map(String::from),
                    unverified: ov.unverified,
                    following: t
                        .guidance
                        .as_ref()
                        .filter(|g| g.family == fam_id)
                        .map(|g| g.follows),
                });
            }
        }
        out
    }

    /// What the thing's page shows from the library.
    pub(crate) fn maker_parts(
        &self,
        item: &Item,
        tasks: &[Task],
    ) -> (Option<MakerOut>, Option<MatchOut>) {
        match item.family.as_deref() {
            Some(NO_FAMILY) => (None, None),
            Some(id) => {
                let lib = &self.appliances().lib;
                match lib.family(id) {
                    Some(f) => (
                        Some(MakerOut {
                            family_id: id.to_string(),
                            in_library: true,
                            family: Some(self.family_out(id, f)),
                            guidance: self.guidance_for(item, f, tasks),
                            other_kind: item.template.is_some()
                                && lib.template_of(f) != item.template,
                        }),
                        None,
                    ),
                    None => (
                        Some(MakerOut {
                            family_id: id.to_string(),
                            in_library: false,
                            family: None,
                            guidance: vec![],
                            other_kind: false,
                        }),
                        None,
                    ),
                }
            }
            None => {
                let make = item.make.as_deref().unwrap_or("");
                if make.trim().is_empty() {
                    return (None, None);
                }
                let m = self.match_out(
                    make,
                    item.model.as_deref().unwrap_or(""),
                    item.template.as_deref(),
                );
                (None, (!m.families.is_empty()).then_some(m))
            }
        }
    }

    fn check_family_id(&self, id: &str, template: Option<&str>) -> Result<(), ApiError> {
        let lib = &self.appliances().lib;
        let f = lib.family(id).ok_or_else(|| {
            ApiError::invalid(format!("the appliance library has no family '{id}'"))
        })?;
        if let (Some(t), Some(ft)) = (template, lib.template_of(f))
            && t != ft
        {
            return Err(ApiError::invalid(format!(
                "{} is a different kind of thing from this one",
                f.name
            )));
        }
        Ok(())
    }

    /// Check a family chosen while adding a thing (D-149).
    pub(crate) fn family_for_new(
        &self,
        family: Option<String>,
        template: &str,
    ) -> Result<Option<String>, ApiError> {
        match family.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(NO_FAMILY) => Ok(Some(NO_FAMILY.into())),
            Some(id) => {
                self.check_family_id(id, Some(template))?;
                Ok(Some(id.to_string()))
            }
        }
    }

    fn set_family(&mut self, a: SetFamilyArgs) -> Result<ItemDetail, ApiError> {
        let id: ItemId = parse_id(&a.item_id, "item")?;
        let mut it: Item = self.store.fetch(&id.to_string())?;
        it.family = match a.family.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(NO_FAMILY) => Some(NO_FAMILY.into()),
            Some(fid) => {
                self.check_family_id(fid, it.template.as_deref())?;
                Some(fid.to_string())
            }
        };
        self.store.put(&it)?;
        self.item_detail(&a.item_id)
    }

    fn choose_guidance(&mut self, a: GuidanceArgs) -> Result<ItemDetail, ApiError> {
        let mut t = self.task(&a.task_id)?;
        let item_id = t
            .item
            .ok_or_else(|| ApiError::invalid("this task is not about a thing"))?;
        let item: Item = self.store.fetch(&item_id.to_string())?;
        let fam_id = item
            .family
            .clone()
            .filter(|f| f != NO_FAMILY)
            .ok_or_else(|| ApiError::invalid("this thing has no model family"))?;
        let r = t
            .template
            .clone()
            .ok_or_else(|| ApiError::invalid("this task did not come from a template"))?;
        let generic = self
            .library
            .resolve(&r.template)?
            .tasks
            .into_iter()
            .find(|x| x.key == r.task)
            .ok_or_else(|| not_found("the generic task is gone"))?;
        let all_tasks: Vec<Task> = self.store.list_by("item", &item_id.to_string())?;
        let offered = {
            let lib = &self.appliances().lib;
            let f = lib
                .family(&fam_id)
                .ok_or_else(|| not_found("the appliance library no longer lists this family"))?;
            self.guidance_for(&item, f, &all_tasks)
                .into_iter()
                .any(|g| g.task_id == t.id.to_string())
        };
        if !offered {
            return Err(ApiError::invalid(
                "the manufacturer has no different advice for this task",
            ));
        }
        let lib = &self.appliances().lib;
        let f = lib.family(&fam_id).expect("checked above");
        let ov = f
            .tasks
            .iter()
            .find(|o| o.key == r.task)
            .expect("offered above");
        let brand = lib
            .brands
            .get(&f.brand)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        let (src, link) = match a.follow {
            Follows::Maker => (merged(&generic, ov, &brand), Some(ov.source.clone())),
            Follows::Generic => (generic, None),
        };
        t.title = src.title;
        t.priority = src.priority;
        t.schedule = src.schedule;
        t.window = src.window;
        t.steps = src.steps;
        t.minutes = src.minutes;
        t.why = Some(Why {
            text: src.why,
            source: src.source,
            link,
        });
        t.guidance = Some(Guidance {
            family: fam_id,
            follows: a.follow,
        });
        self.store.put(&t)?;
        self.item_detail(&item_id.to_string())
    }

    /// Check that `url` is one of the thing's family's manual links, and
    /// say which hosts a download may use (the brand's own domains).
    pub(crate) fn manual_source(&self, a: &ManualArgs) -> Result<ManualSource, ApiError> {
        let id: ItemId = parse_id(&a.item_id, "item")?;
        let item: Item = self.store.fetch(&id.to_string())?;
        let fam_id = item
            .family
            .filter(|f| f != NO_FAMILY)
            .ok_or_else(|| ApiError::invalid("this thing has no model family"))?;
        let lib = &self.appliances().lib;
        let f = lib
            .family(&fam_id)
            .ok_or_else(|| not_found("the appliance library no longer lists this family"))?;
        let m = f.manuals.iter().find(|m| m.url == a.url).ok_or_else(|| {
            ApiError::invalid("that link is not one of this family's manual links")
        })?;
        if m.kind != "manual-pdf" {
            return Err(ApiError::invalid(
                "that link is a web page, not a PDF; open it in the browser instead",
            ));
        }
        let domains = lib.brand_domains(f);
        let host = appliances::check::https_host(&m.url).map_err(ApiError::invalid)?;
        if !appliances::check::host_allowed(&host, &domains) {
            return Err(ApiError::invalid(
                "that link is not on the manufacturer's own site",
            ));
        }
        let brand = lib
            .brands
            .get(&f.brand)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        Ok(ManualSource {
            url: m.url.clone(),
            name: manual_name(f, &brand, &m.url),
            domains,
        })
    }

    /// Keep a downloaded manual as one of the thing's files (D-151): a
    /// PDF, from one of its family's manual links, at most 25 MB.
    pub(crate) fn attach_manual(&mut self, a: Value, bytes: &[u8]) -> Result<Value, ApiError> {
        let a: ManualArgs = args(a)?;
        let src = self.manual_source(&a)?;
        if !photo::is_pdf(bytes) {
            return Err(ApiError::invalid(
                "the download is not a PDF; nothing was saved",
            ));
        }
        if bytes.len() > MAX_MANUAL_BYTES {
            return Err(ApiError::invalid("the manual is too large to keep"));
        }
        let att = self.store.add_attachment(
            self.blobs.as_mut(),
            NewFile {
                about: EntityRef {
                    kind: EntityKind::Item,
                    id: a.item_id.clone(),
                },
                kind: AttachmentKind::Manual,
                name: src.name,
                media_type: "application/pdf".into(),
                bytes,
                thumb: None,
                width: None,
                height: None,
                sensitivity: Sensitivity::Normal,
                source: Some(src.url),
            },
        )?;
        ok(records::attachment_out(&att))
    }

    fn supplies(&self) -> Vec<SupplyOut> {
        let mut out = Vec::new();
        let Ok(items) = self.store.list::<Item>() else {
            return out;
        };
        for it in items {
            let Some(fid) = it.family.as_deref().filter(|f| *f != NO_FAMILY) else {
                continue;
            };
            let Some(f) = self.appliances().lib.family(fid) else {
                continue;
            };
            for c in self.family_out(fid, f).consumables {
                out.push(SupplyOut {
                    item_id: it.id.to_string(),
                    item_name: it.name.clone(),
                    family_id: fid.to_string(),
                    consumable: c,
                });
            }
        }
        out
    }
}

// -- downloading a manual (native only; the caller supplies the transport) --

/// An answer: status, headers, body.
pub type Fetched = (u16, Vec<(String, String)>, Vec<u8>);

/// One HTTP GET, without following redirects.
pub trait Fetch {
    /// Returns (status, headers, body). The body may be cut at `limit + 1`
    /// bytes. `Err` means no answer (offline, TLS, timeout).
    fn get(&mut self, url: &str, limit: usize) -> Result<Fetched, String>;
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Download a manual PDF from the manufacturer (D-151): https only, every
/// hop (and every redirect) on the brand's own domains, at most
/// [`MAX_MANUAL_REDIRECTS`] redirects and [`MAX_MANUAL_BYTES`] bytes, and
/// the answer must be a PDF (by its first bytes; the content type, when
/// given, must not say otherwise). Nothing about the household is sent:
/// the request is the public URL and a generic user agent.
pub fn download_manual(f: &mut dyn Fetch, src: &ManualSource) -> Result<Vec<u8>, ApiError> {
    let mut url = src.url.clone();
    for _ in 0..=MAX_MANUAL_REDIRECTS {
        let host = appliances::check::https_host(&url)
            .map_err(|e| ApiError::invalid(format!("the manual's link {e}")))?;
        if !appliances::check::host_allowed(&host, &src.domains) {
            return Err(ApiError::invalid(format!(
                "the manufacturer's site sent the download to {host}, which is not its own site; open the page in the browser instead"
            )));
        }
        let (status, headers, body) = f.get(&url, MAX_MANUAL_BYTES).map_err(|e| ApiError {
            code: "offline".into(),
            message: format!("could not reach the manufacturer's site ({e})"),
        })?;
        match status {
            301 | 302 | 303 | 307 | 308 => {
                let to = header(&headers, "location").ok_or_else(|| {
                    ApiError::invalid("the manufacturer's site redirected nowhere")
                })?;
                url = if to.starts_with("https://") {
                    to.to_string()
                } else if to.starts_with('/') && !to.starts_with("//") {
                    format!("https://{host}{to}")
                } else {
                    return Err(ApiError::invalid(
                        "the manufacturer's site redirected to a link Housewright does not follow",
                    ));
                };
                continue;
            }
            200 => {}
            s => {
                return Err(ApiError::invalid(format!(
                    "the manufacturer's site answered {s}; the manual may have moved. Try the manufacturer's page instead."
                )));
            }
        }
        if body.len() > MAX_MANUAL_BYTES {
            return Err(ApiError::invalid(
                "the manual is larger than 25 MB; open it in the browser instead",
            ));
        }
        let ct = header(&headers, "content-type")
            .unwrap_or("")
            .to_ascii_lowercase();
        let ct_ok = ct.is_empty()
            || ct.starts_with("application/pdf")
            || ct.starts_with("application/octet-stream")
            || ct.starts_with("binary/octet-stream")
            || ct.starts_with("application/x-pdf");
        if !ct_ok || !photo::is_pdf(&body) {
            return Err(ApiError::invalid(
                "the manufacturer's site did not send a PDF (it may want you to open the page first); open it in the browser instead",
            ));
        }
        return Ok(body);
    }
    Err(ApiError::invalid(
        "the manufacturer's site redirected too many times",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    type Route = (String, u16, Vec<(String, String)>, Vec<u8>);
    /// Answers by URL, and the URLs asked for.
    struct Fake(Vec<Route>, Vec<String>);
    impl Fetch for Fake {
        fn get(&mut self, url: &str, _limit: usize) -> Result<Fetched, String> {
            self.1.push(url.to_string());
            let i = self
                .0
                .iter()
                .position(|(u, ..)| u == url)
                .ok_or("no route")?;
            let (_, s, h, b) = self.0[i].clone();
            Ok((s, h, b))
        }
    }

    fn src() -> ManualSource {
        ManualSource {
            url: "https://www.acme.com/m.pdf".into(),
            name: "m.pdf".into(),
            domains: vec!["acme.com".into()],
        }
    }
    fn pdf() -> Vec<u8> {
        b"%PDF-1.7\n...".to_vec()
    }
    fn ct(v: &str) -> Vec<(String, String)> {
        vec![("Content-Type".into(), v.into())]
    }

    #[test]
    fn downloads_a_pdf_and_follows_same_site_redirects() {
        let mut f = Fake(
            vec![
                (
                    "https://www.acme.com/m.pdf".into(),
                    302,
                    vec![("Location".into(), "/docs/m.pdf".into())],
                    vec![],
                ),
                (
                    "https://www.acme.com/docs/m.pdf".into(),
                    301,
                    vec![("location".into(), "https://cdn.acme.com/m.pdf".into())],
                    vec![],
                ),
                (
                    "https://cdn.acme.com/m.pdf".into(),
                    200,
                    ct("application/pdf"),
                    pdf(),
                ),
            ],
            vec![],
        );
        assert_eq!(download_manual(&mut f, &src()).unwrap(), pdf());
        assert_eq!(f.1.len(), 3);
    }

    #[test]
    fn refuses_other_sites_html_and_errors() {
        let mut f = Fake(
            vec![(
                "https://www.acme.com/m.pdf".into(),
                302,
                vec![("Location".into(), "https://evil.example/m.pdf".into())],
                vec![],
            )],
            vec![],
        );
        assert!(
            download_manual(&mut f, &src())
                .unwrap_err()
                .message
                .contains("not its own site")
        );
        assert_eq!(f.1.len(), 1, "never asked the other site");

        let mut f = Fake(
            vec![(
                "https://www.acme.com/m.pdf".into(),
                302,
                vec![("Location".into(), "http://www.acme.com/m.pdf".into())],
                vec![],
            )],
            vec![],
        );
        assert!(download_manual(&mut f, &src()).is_err());

        let mut f = Fake(
            vec![(
                "https://www.acme.com/m.pdf".into(),
                200,
                ct("text/html"),
                b"<html>".to_vec(),
            )],
            vec![],
        );
        assert!(
            download_manual(&mut f, &src())
                .unwrap_err()
                .message
                .contains("did not send a PDF")
        );

        // A PDF content type but HTML bytes.
        let mut f = Fake(
            vec![(
                "https://www.acme.com/m.pdf".into(),
                200,
                ct("application/pdf"),
                b"<html>".to_vec(),
            )],
            vec![],
        );
        assert!(download_manual(&mut f, &src()).is_err());

        let mut f = Fake(
            vec![("https://www.acme.com/m.pdf".into(), 404, vec![], vec![])],
            vec![],
        );
        assert!(
            download_manual(&mut f, &src())
                .unwrap_err()
                .message
                .contains("404")
        );

        let big = {
            let mut b = pdf();
            b.resize(MAX_MANUAL_BYTES + 1, 0);
            b
        };
        let mut f = Fake(
            vec![(
                "https://www.acme.com/m.pdf".into(),
                200,
                ct("application/pdf"),
                big,
            )],
            vec![],
        );
        assert!(
            download_manual(&mut f, &src())
                .unwrap_err()
                .message
                .contains("25 MB")
        );

        // A redirect loop.
        let mut f = Fake(
            vec![(
                "https://www.acme.com/m.pdf".into(),
                302,
                vec![("Location".into(), "/m.pdf".into())],
                vec![],
            )],
            vec![],
        );
        assert!(
            download_manual(&mut f, &src())
                .unwrap_err()
                .message
                .contains("too many")
        );
    }
}
