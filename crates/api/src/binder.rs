//! Phase 5 commands: the house binder (D-40 to D-43, D-80 to D-86),
//! "quirks only you know" notes, and the contractors the binder lists.

use std::collections::BTreeMap;

use housewright_binder::{self as binder, Edition, Options, Paper, Volume};
use housewright_core::facts;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::priority::PriorityClass;
use housewright_core::schedule::{Interval, Schedule};
use housewright_media::photo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

/// The long edge of a photo in the binder, in pixels: about 3 inches at
/// 300 dpi, which is as large as the layout ever prints one.
const PRINT_EDGE: u32 = 900;
const PRINT_QUALITY: u8 = 78;

/// The reminder task's template reference, so it can be found again.
const REMINDER_TEMPLATE: &str = "housewright";
const REMINDER_TASK: &str = "reprint-binder";

#[derive(Deserialize)]
pub struct BinderArgs {
    #[serde(default)]
    pub property_id: Option<String>,
    /// The handover edition's choices, so its change markers compare
    /// like with like.
    #[serde(default)]
    pub include_costs: Option<bool>,
    #[serde(default)]
    pub include_notes: Option<bool>,
}

#[derive(Deserialize)]
pub struct BinderPdfArgs {
    pub edition: String,
    /// `main` (default) or `sealed`.
    #[serde(default)]
    pub volume: Option<String>,
    /// Print only these sections (a reprint).
    #[serde(default)]
    pub sections: Option<Vec<String>>,
    /// Print only the sections that changed since the last print.
    #[serde(default)]
    pub only_changed: bool,
    #[serde(default)]
    pub include_costs: Option<bool>,
    #[serde(default)]
    pub include_notes: Option<bool>,
    #[serde(default)]
    pub paper: Option<Paper>,
    #[serde(default)]
    pub property_id: Option<String>,
    /// Record this as printed (default true). Previews pass false.
    #[serde(default)]
    pub record: Option<bool>,
    /// The recovery key, only while it is on screen at setup: printed on
    /// the sealed volume's recovery page (D-91). Never stored.
    #[serde(default)]
    pub recovery_key: Option<String>,
}

#[derive(Serialize)]
pub struct EditionOut {
    pub edition: Edition,
    pub label: String,
    pub description: String,
    pub has_sealed: bool,
    #[serde(flatten)]
    pub status: binder::track::EditionStatus,
}

#[derive(Serialize)]
pub struct BinderOut {
    pub property_name: String,
    pub editions: Vec<EditionOut>,
    /// Where the sealed volume is kept, if written down.
    pub sealed_location: Option<String>,
    /// Only the owner may print the sealed volume (D-23).
    pub can_print_sealed: bool,
    /// The yearly "Reprint the house binder" task, if it exists.
    pub reminder: Option<ReminderOut>,
    pub paper: Paper,
}

#[derive(Serialize)]
pub struct ReminderOut {
    pub task_id: String,
    pub active: bool,
}

#[derive(Deserialize)]
pub struct ReminderArgs {
    pub on: bool,
}

#[derive(Deserialize)]
pub struct PaperArgs {
    pub paper: Paper,
}

#[derive(Deserialize)]
pub struct NotesArgs {
    /// `property`, `room` or `item`.
    pub about_kind: String,
    pub about_id: String,
}

#[derive(Deserialize)]
pub struct AddNoteArgs {
    pub about_kind: String,
    pub about_id: String,
    pub text: String,
    /// Absent: decided from the words (a spare key's hiding place is sensitive).
    #[serde(default)]
    pub sensitive: Option<bool>,
}

#[derive(Serialize)]
pub struct NoteOut {
    pub id: String,
    /// `None` when sensitive and the viewer may not see it.
    pub text: Option<String>,
    pub sensitive: bool,
    pub at: jiff::Timestamp,
}

#[derive(Serialize, Deserialize)]
pub struct ContractorIo {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub trades: Vec<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Notes go in the sealed volume only.
    #[serde(default)]
    pub notes_private: bool,
    #[serde(default)]
    pub emergency: bool,
}

fn note_kind(s: &str) -> Result<EntityKind, ApiError> {
    match s {
        "property" => Ok(EntityKind::Property),
        "room" => Ok(EntityKind::Room),
        "item" => Ok(EntityKind::Item),
        other => Err(ApiError::invalid(format!(
            "notes can be about the house, a room or a thing, not '{other}'"
        ))),
    }
}

fn edition_text(e: Edition) -> &'static str {
    match e {
        Edition::Emergency => {
            "One page: shutoffs, who to call, and what to do for a leak, a gas smell, no power or a carbon monoxide alarm. Also page 1 of every binder."
        }
        Edition::Full => {
            "Everything about running the house. Codes and account numbers go in a separate sealed volume."
        }
        Edition::Handover => {
            "For a buyer or new tenant: systems, ages, service history, documents and warranties. Names become \"previous owner\"; no sealed volume. Your notes about things are included: check them for anything personal."
        }
        Edition::Succession => {
            "For someone inheriting the house: the household binder plus accounts and utilities to transfer (in the sealed volume)."
        }
    }
}

fn blank(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

impl Api {
    pub(crate) fn binder_call(&mut self, cmd: &str, a: Value) -> Option<Result<Value, ApiError>> {
        Some(match cmd {
            "binder" => args(a).and_then(|a: BinderArgs| {
                let mut h = Options::for_edition(Edition::Handover);
                if let Some(c) = a.include_costs {
                    h.include_costs = c;
                }
                if let Some(n) = a.include_notes {
                    h.include_notes = n;
                }
                ok(self.binder_status_with(a.property_id, h)?)
            }),
            "set_sealed_location" => {
                args(a).and_then(|a: TextArgs| ok(self.set_sealed_location(&a.text)?))
            }
            "set_binder_reminder" => {
                args(a).and_then(|a: ReminderArgs| ok(self.set_binder_reminder(a.on)?))
            }
            "set_paper" => args(a).and_then(|a: PaperArgs| {
                self.store.set_meta(
                    "binder_paper",
                    match a.paper {
                        Paper::Letter => "letter",
                        Paper::A4 => "a4",
                    },
                )?;
                ok(self.binder_status(None)?)
            }),
            "notes" => args(a).and_then(|a: NotesArgs| ok(self.notes(a)?)),
            "add_note" => args(a).and_then(|a: AddNoteArgs| ok(self.add_note(a)?)),
            "delete_note" => args(a).and_then(|a: IdArgs| {
                let id: NoteId = parse_id(&a.id, "note")?;
                let _: Note = self.store.fetch(&id.to_string())?;
                self.store.delete(EntityKind::Note, &id.to_string())?;
                ok(())
            }),
            "record_binder_print" => args(a).and_then(|r: binder::bundle::Receipt| {
                self.record_print(&r)?;
                ok(self.binder_status(None)?)
            }),
            "contractors" => ok(self.contractors()),
            "save_contractor" => args(a).and_then(|a: ContractorIo| ok(self.save_contractor(a)?)),
            "delete_contractor" => args(a).and_then(|a: IdArgs| {
                let id: ContractorId = parse_id(&a.id, "contractor")?;
                let _: Contractor = self.store.fetch(&id.to_string())?;
                self.store.delete(EntityKind::Contractor, &id.to_string())?;
                ok(())
            }),
            _ => return None,
        })
    }

    fn paper(&self) -> Result<Paper, ApiError> {
        Ok(match self.store.meta("binder_paper")?.as_deref() {
            Some("a4") => Paper::A4,
            _ => Paper::Letter,
        })
    }

    fn binder_property(&self, id: Option<String>) -> Result<Option<PropertyId>, ApiError> {
        id.map(|p| parse_id::<PropertyId>(&p, "property"))
            .transpose()
    }

    fn reminder_task(&self) -> Result<Option<Task>, ApiError> {
        Ok(self.store.list::<Task>()?.into_iter().find(|t| {
            t.template
                .as_ref()
                .is_some_and(|r| r.template == REMINDER_TEMPLATE && r.task == REMINDER_TASK)
        }))
    }

    fn binder_status(&self, property: Option<String>) -> Result<BinderOut, ApiError> {
        self.binder_status_with(property, Options::for_edition(Edition::Handover))
    }

    fn binder_status_with(
        &self,
        property: Option<String>,
        handover: Options,
    ) -> Result<BinderOut, ApiError> {
        let pid = self.binder_property(property)?;
        let data = binder::gather(&self.store, pid, self.store.now())?;
        let prints: Vec<BinderPrint> = self
            .store
            .list::<BinderPrint>()?
            .into_iter()
            .filter(|p| p.property == data.property.id)
            .collect();
        let can_sealed = self.me()?.role.can_see_sensitive();
        let mut editions = Vec::new();
        for e in Edition::ALL {
            let opts = if e == Edition::Handover {
                handover
            } else {
                Options::for_edition(e)
            };
            let b = binder::build(&data, e, opts);
            let mut docs = vec![&b.main];
            if can_sealed && let Some(s) = &b.sealed {
                docs.push(s);
            }
            editions.push(EditionOut {
                edition: e,
                label: e.label().into(),
                description: edition_text(e).into(),
                has_sealed: e.has_sealed(),
                status: binder::track::status(e, &docs, &prints),
            });
        }
        let reminder = self.reminder_task()?.map(|t| ReminderOut {
            task_id: t.id.to_string(),
            active: t.active,
        });
        Ok(BinderOut {
            property_name: data.property.name.clone(),
            sealed_location: data.sealed_location().map(str::to_string),
            editions,
            can_print_sealed: can_sealed,
            reminder,
            paper: self.paper()?,
        })
    }

    fn set_sealed_location(&mut self, text: &str) -> Result<BinderOut, ApiError> {
        let p = self.first_property()?;
        let text = text.trim();
        let existing: Option<Fact> = self
            .store
            .list_by::<Fact>("property", &p.id.to_string())?
            .into_iter()
            .find(|f| f.key.as_deref() == Some(binder::SEALED_LOCATION_KEY));
        match (existing, text.is_empty()) {
            (Some(f), true) => self.store.delete(EntityKind::Fact, &f.id.to_string())?,
            (None, true) => {}
            (existing, false) => {
                // Normal on purpose: the main binder must be able to say
                // where the sealed volume is (D-41).
                let f = Fact {
                    pin: None,
                    id: existing.as_ref().map(|f| f.id).unwrap_or_default(),
                    property: p.id,
                    category: "other".into(),
                    label: "Where the sealed volume is kept".into(),
                    value: text.to_string(),
                    sensitivity: Sensitivity::Normal,
                    key: Some(binder::SEALED_LOCATION_KEY.into()),
                    room: None,
                };
                self.store.put(&f)?;
            }
        }
        self.binder_status(None)
    }

    /// The yearly reminder (P5, P10): never switched on by itself. It is
    /// created switched off after the first print, so it appears as an
    /// ordinary suggestion; this switches it on or off.
    fn set_binder_reminder(&mut self, on: bool) -> Result<BinderOut, ApiError> {
        let mut t = match self.reminder_task()? {
            Some(t) => t,
            None => self.make_reminder_task()?,
        };
        t.active = on;
        if on {
            t.paused = false;
            t.not_applicable = false;
        }
        self.store.put(&t)?;
        self.binder_status(None)
    }

    fn make_reminder_task(&mut self) -> Result<Task, ApiError> {
        let p = self.first_property()?;
        let today = self.today_in(&p.time_zone)?;
        let t = Task {
            id: TaskId::new(),
            property: p.id,
            item: None,
            room: None,
            title: "Reprint the house binder".into(),
            owner: self.me()?.id,
            priority: PriorityClass::Cosmetic,
            schedule: Schedule::Fixed {
                every: Interval::years(1),
                anchor: None,
            },
            window: None,
            start: add_days(today, 365),
            steps: vec![
                "Open Home, then House binder.".into(),
                "Choose \"Print changed sections\".".into(),
                "Swap the new pages into the binder and recycle the old ones.".into(),
            ],
            why: Some(Why {
                text: "The binder helps most when it is right. Only the sections that changed need printing.".into(),
                source: "Housewright".into(),
            }),
            minutes: Some(15),
            completion_fields: vec![],
            template: Some(TemplateTaskRef {
                template: REMINDER_TEMPLATE.into(),
                task: REMINDER_TASK.into(),
            }),
            active: false,
            paused: false,
            not_applicable: false,
        };
        self.store.put(&t)?;
        Ok(t)
    }

    /// Build, check and prepare one volume for typesetting.
    fn binder_bundle(
        &mut self,
        a: BinderPdfArgs,
    ) -> Result<(binder::bundle::Bundle, bool), ApiError> {
        let edition = Edition::parse(&a.edition)
            .ok_or_else(|| ApiError::invalid(format!("no binder edition '{}'", a.edition)))?;
        let volume = match a.volume.as_deref().unwrap_or("main") {
            "main" => Volume::Main,
            "sealed" => Volume::Sealed,
            v => return Err(ApiError::invalid(format!("no volume '{v}'"))),
        };
        let me = self.me()?;
        if volume == Volume::Sealed && !me.role.can_see_sensitive() {
            return Err(ApiError::invalid(
                "only the household's owner can print the sealed volume".to_string(),
            ));
        }
        if !me.role.can_edit_plan() {
            return Err(ApiError::invalid(
                "ask the household's owner to print the binder".to_string(),
            ));
        }
        let mut opts = Options::for_edition(edition);
        if let Some(c) = a.include_costs {
            opts.include_costs = c;
        }
        if let Some(n) = a.include_notes {
            opts.include_notes = n;
        }
        let pid = self.binder_property(a.property_id)?;
        let mut data = binder::gather(&self.store, pid, self.store.now())?;
        if let Some(k) = a.recovery_key.as_deref() {
            let key = housewright_crypto::recovery::RecoveryKey::parse(k)
                .map_err(|e| ApiError::invalid(e.to_string()))?;
            let server = match &data.recovery {
                binder::Recovery::KeptSeparately { server } => server.clone(),
                _ => return Err(ApiError::invalid("sync is not set up on this device")),
            };
            data.recovery = binder::Recovery::Key {
                server,
                key: key.display().to_string(),
            };
        }
        let b = binder::build(&data, edition, opts);
        let doc = match volume {
            Volume::Main => {
                binder::check_main(&data, &b.main)
                    .map_err(|e| ApiError::internal(e.to_string()))?;
                b.main
            }
            Volume::Sealed => b.sealed.ok_or_else(|| {
                ApiError::invalid(format!("the {} has no sealed volume", edition.label()))
            })?,
        };
        let doc = if a.only_changed {
            let prints: Vec<BinderPrint> = self.store.list()?;
            let st = binder::track::status(edition, &[&doc], &prints);
            let keys: Vec<String> = st
                .sections
                .into_iter()
                .filter(|s| s.changed)
                .map(|s| s.key)
                .collect();
            if keys.is_empty() {
                return Err(ApiError::invalid(
                    "nothing in this volume has changed since it was printed".to_string(),
                ));
            }
            doc.only(&keys)
        } else if let Some(keys) = &a.sections {
            doc.only(keys)
        } else {
            doc
        };
        if doc.sections.is_empty() {
            return Err(ApiError::invalid("no such sections".to_string()));
        }
        let images = self.print_images(&doc)?;
        let paper = match a.paper {
            Some(p) => p,
            None => self.paper()?,
        };
        let bundle = binder::prepare(&doc, data.property.id, data.today, paper, &images);
        Ok((bundle, a.record.unwrap_or(true)))
    }

    /// Record a print (D-82), and after the first one create the yearly
    /// reminder, switched off (D-87).
    fn record_print(&mut self, r: &binder::bundle::Receipt) -> Result<(), ApiError> {
        if Edition::parse(&r.edition).is_none() || !matches!(r.volume.as_str(), "main" | "sealed") {
            return Err(ApiError::invalid("not a binder receipt".to_string()));
        }
        let _: Property = self.store.fetch(&r.property.to_string())?;
        let me = self.me()?;
        self.store.append(&BinderPrint {
            id: BinderPrintId::new(),
            property: r.property,
            edition: r.edition.clone(),
            volume: r.volume.clone(),
            by: me.id,
            at: self.store.now(),
            printed_on: r.printed_on,
            sections: r.sections.clone(),
        })?;
        if self.reminder_task()?.is_none() {
            self.make_reminder_task()?;
        }
        Ok(())
    }

    /// Typeset in this process (Android, desktop).
    #[cfg(feature = "typeset")]
    fn binder_pdf(&mut self, a: BinderPdfArgs) -> Result<Vec<u8>, ApiError> {
        let (bundle, record) = self.binder_bundle(a)?;
        let r = binder::render::render(&bundle.source, &bundle.files, bundle.receipt.printed_on)
            .map_err(|e| ApiError::internal(e.to_string()))?;
        if record {
            self.record_print(&bundle.receipt)?;
        }
        Ok(r.pdf)
    }

    #[cfg(not(feature = "typeset"))]
    fn binder_pdf(&mut self, _a: BinderPdfArgs) -> Result<Vec<u8>, ApiError> {
        Err(ApiError::invalid(
            "this build typesets the binder in its separate module: use binder_bundle".to_string(),
        ))
    }

    /// Photos for a document, shrunk for print (D-85).
    fn print_images(&self, doc: &binder::Document) -> Result<BTreeMap<String, Vec<u8>>, ApiError> {
        let mut out = BTreeMap::new();
        for file in doc.photo_files() {
            let Some(sha) = file
                .strip_prefix("img/")
                .and_then(|f| f.strip_suffix(".jpg"))
            else {
                continue;
            };
            let Some(bytes) = self.blobs.get(sha)? else {
                continue;
            };
            let Ok(img) = photo::decode(&bytes) else {
                continue;
            };
            let small = photo::fit(&img, PRINT_EDGE);
            if let Ok(jpg) = photo::encode_jpeg(&small, PRINT_QUALITY) {
                out.insert(file, jpg);
            }
        }
        Ok(out)
    }

    fn notes(&self, a: NotesArgs) -> Result<Vec<NoteOut>, ApiError> {
        let kind = note_kind(&a.about_kind)?;
        let see = self.me()?.role.can_see_sensitive();
        let mut v: Vec<Note> = self
            .store
            .list::<Note>()?
            .into_iter()
            .filter(|n| n.about.kind == kind && n.about.id == a.about_id)
            .collect();
        v.sort_by_key(|n| (n.at, n.id));
        Ok(v.into_iter()
            .map(|n| {
                let sensitive = n.text.sensitivity == Sensitivity::Sensitive;
                NoteOut {
                    id: n.id.to_string(),
                    text: (!sensitive || see).then_some(n.text.value),
                    sensitive,
                    at: n.at,
                }
            })
            .collect())
    }

    fn add_note(&mut self, a: AddNoteArgs) -> Result<Vec<NoteOut>, ApiError> {
        let kind = note_kind(&a.about_kind)?;
        match kind {
            EntityKind::Property => {
                self.store.fetch::<Property>(
                    &parse_id::<PropertyId>(&a.about_id, "property")?.to_string(),
                )?;
            }
            EntityKind::Room => {
                self.store
                    .fetch::<Room>(&parse_id::<RoomId>(&a.about_id, "room")?.to_string())?;
            }
            _ => {
                self.store
                    .fetch::<Item>(&parse_id::<ItemId>(&a.about_id, "item")?.to_string())?;
            }
        }
        let text = a.text.trim().to_string();
        if text.is_empty() {
            return Err(ApiError::invalid("write the note".to_string()));
        }
        let sensitivity = match a.sensitive {
            Some(true) => Sensitivity::Sensitive,
            Some(false) => Sensitivity::Normal,
            None => facts::default_sensitivity("other", &text),
        };
        self.store.append(&Note {
            id: NoteId::new(),
            about: EntityRef {
                kind,
                id: a.about_id.clone(),
            },
            by: self.me()?.id,
            at: self.store.now(),
            text: Flagged {
                value: text,
                sensitivity,
            },
        })?;
        self.notes(NotesArgs {
            about_kind: a.about_kind,
            about_id: a.about_id,
        })
    }

    fn contractors(&self) -> Vec<ContractorIo> {
        let see = self
            .me()
            .map(|m| m.role.can_see_sensitive())
            .unwrap_or(false);
        let mut v: Vec<Contractor> = self.store.list().unwrap_or_default();
        v.sort_by_key(|c| c.name.to_lowercase());
        v.into_iter()
            .map(|c| {
                let private = c
                    .notes
                    .as_ref()
                    .is_some_and(|n| n.sensitivity == Sensitivity::Sensitive);
                ContractorIo {
                    id: Some(c.id.to_string()),
                    name: c.name,
                    trades: c.trades,
                    phone: c.phone,
                    email: c.email,
                    notes: c.notes.filter(|_| !private || see).map(|n| n.value),
                    notes_private: private,
                    emergency: c.emergency,
                }
            })
            .collect()
    }

    fn save_contractor(&mut self, a: ContractorIo) -> Result<Vec<ContractorIo>, ApiError> {
        if !self.me()?.role.can_edit_plan() {
            return Err(ApiError::invalid(
                "only household members can do this".to_string(),
            ));
        }
        let name = a.name.trim().to_string();
        if name.is_empty() {
            return Err(ApiError::invalid("give it a name".to_string()));
        }
        let existing: Option<Contractor> = match &a.id {
            Some(id) => Some(
                self.store
                    .fetch(&parse_id::<ContractorId>(id, "contractor")?.to_string())?,
            ),
            None => None,
        };
        let notes = blank(a.notes).map(|n| Flagged {
            value: n,
            sensitivity: if a.notes_private {
                Sensitivity::Sensitive
            } else {
                Sensitivity::Normal
            },
        });
        let c = Contractor {
            id: existing.as_ref().map(|c| c.id).unwrap_or_default(),
            name,
            trades: a
                .trades
                .into_iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect(),
            phone: blank(a.phone),
            email: blank(a.email),
            licence: existing.as_ref().and_then(|c| c.licence.clone()),
            insurance: existing.as_ref().and_then(|c| c.insurance.clone()),
            // Kept as it was: the rating is private and not edited here (D-28).
            private_rating: existing.as_ref().and_then(|c| c.private_rating),
            notes,
            emergency: a.emergency,
        };
        self.store.put(&c)?;
        Ok(self.contractors())
    }

    pub(crate) fn binder_download(
        &mut self,
        cmd: &str,
        a: Value,
    ) -> Option<Result<Vec<u8>, ApiError>> {
        match cmd {
            "binder_pdf" => Some(args(a).and_then(|a: BinderPdfArgs| self.binder_pdf(a))),
            "binder_bundle" => Some(args(a).and_then(|a: BinderPdfArgs| {
                let (b, _) = self.binder_bundle(a)?;
                b.to_zip().map_err(|e| ApiError::internal(e.to_string()))
            })),
            _ => None,
        }
    }
}
