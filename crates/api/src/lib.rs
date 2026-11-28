//! One command surface over `housewright-core`, shared by every front end.
//!
//! The Tauri app (Android, desktop) and the WebAssembly build (web page) both
//! call [`Api::call`] with a command name and JSON arguments and get JSON back.
//! That keeps the TypeScript side to a single, typed list of commands
//! (`app/src/lib/api/types.ts`) whichever backend is running, and means the
//! Android notification receiver records a completion through exactly the same
//! code as the "Done" button.
//!
//! Licensed AGPL-3.0-only.

mod binder;
mod dto;
mod records;
mod templates;

use std::collections::BTreeMap;

use housewright_core::audit::audit_log;
use housewright_core::export;
use housewright_core::fields::{CustomValue, FieldType, FieldValue};
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::priority::{self, START_SMALL_DEFAULT, TaskStatus};
use housewright_core::profile::{Profile, starting_count};
use housewright_core::reminders::{self, PlanInput, ReminderPrefs, ReminderText};
use housewright_core::schedule::{Climate, Interval, Schedule, TaskState};
use housewright_core::store::Store;
use housewright_core::template::Library;
use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use dto::*;
pub use records::{DOWNLOAD_COMMANDS, UPLOAD_COMMANDS};

use housewright_core::blobs::{BlobStore, MemoryBlobs};
use housewright_media::ocr::Ocr;

/// Builds the OCR engine the first time label reading is used.
pub type OcrLoader = Box<dyn Fn() -> Result<Ocr, housewright_media::MediaError> + Send>;

/// Every command [`Api::call`] understands. The TypeScript client keeps the
/// same list; a test on each side checks it.
pub const COMMANDS: &[&str] = &[
    "status",
    "init",
    "today",
    "mark_seen",
    "task",
    "complete",
    "annotate",
    "void_completion",
    "snooze_options",
    "snooze",
    "review",
    "set_active",
    "items",
    "item",
    "rooms",
    "add_room",
    "members",
    "templates",
    "add_from_template",
    "start_small",
    "history",
    "set_reminder_prefs",
    "reminder_plan",
    "test_reminder",
    "export",
    "import",
    "climate_lookup",
    "profile",
    "onboard",
    "starting_plan",
    "all_suggested",
    "answer_suggestion",
    "attachments",
    "delete_attachment",
    "update_item",
    "add_field",
    "remove_field",
    "reveal_field",
    "add_details",
    "facts",
    "save_fact",
    "delete_fact",
    "reveal_fact",
    "costs",
    "set_currency",
    "label_sheet",
    "find_label",
    "binder",
    "set_sealed_location",
    "set_binder_reminder",
    "set_paper",
    "notes",
    "add_note",
    "delete_note",
    "contractors",
    "save_contractor",
    "delete_contractor",
];

/// The bundled template library, parsed once.
pub fn bundled_library() -> Result<Library, ApiError> {
    let mut parsed = Vec::new();
    for (file, text) in templates::BUNDLED {
        let t = housewright_core::template::Template::from_toml(text, file)?;
        parsed.push((file.to_string(), t));
    }
    Ok(Library::new(parsed)?)
}

/// A household database plus the bundled templates.
pub struct Api {
    store: Store,
    library: Library,
    /// Attachment bytes (D-71).
    blobs: Box<dyn BlobStore>,
    ocr: Option<Ocr>,
    ocr_loader: Option<OcrLoader>,
}

fn args<T: DeserializeOwned>(v: Value) -> Result<T, ApiError> {
    let v = if v.is_null() { json!({}) } else { v };
    serde_json::from_value(v).map_err(|e| ApiError::invalid(format!("bad arguments: {e}")))
}

fn ok<T: serde::Serialize>(v: T) -> Result<Value, ApiError> {
    serde_json::to_value(v).map_err(|e| ApiError::internal(e.to_string()))
}

fn parse_id<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, ApiError> {
    s.parse()
        .map_err(|_| ApiError::invalid(format!("'{s}' is not a {what} id")))
}

fn add_days(d: Date, n: i64) -> Date {
    d.checked_add(Span::new().days(n)).unwrap_or(d)
}

impl Api {
    /// With attachments kept in memory (tests, and the web's fallback).
    pub fn new(store: Store) -> Result<Self, ApiError> {
        Self::with_blobs(store, Box::new(MemoryBlobs::default()))
    }

    pub fn with_blobs(store: Store, blobs: Box<dyn BlobStore>) -> Result<Self, ApiError> {
        Ok(Self {
            store,
            library: bundled_library()?,
            blobs,
            ocr: None,
            ocr_loader: None,
        })
    }

    /// How to load the label reader when it is first needed (Android: from
    /// models compiled into the app).
    pub fn set_ocr_loader(&mut self, loader: OcrLoader) {
        self.ocr_loader = Some(loader);
    }

    /// Give a loaded label reader (the web worker loads the models itself).
    pub fn set_ocr(&mut self, ocr: Ocr) {
        self.ocr = Some(ocr);
    }

    pub fn has_ocr(&self) -> bool {
        self.ocr.is_some() || self.ocr_loader.is_some()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Direct access to the store, for tests and tools that need to set
    /// something no command offers (a contractor's private rating).
    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// The attachment bytes, for tests and tools.
    pub fn blobs(&self) -> &dyn BlobStore {
        self.blobs.as_ref()
    }

    /// Run one command. `args` is a JSON object (or null for none).
    pub fn call(&mut self, cmd: &str, a: Value) -> Result<Value, ApiError> {
        match cmd {
            "status" => ok(self.status()?),
            "init" => {
                let a: InitArgs = args(a)?;
                ok(self.init(a)?)
            }
            "import" => {
                let a: ImportArgs = args(a)?;
                ok(self.import(&a.json)?)
            }
            _ if self.store.meta("household_id")?.is_none() => Err(ApiError {
                code: "not_initialized".into(),
                message: "set up a household first".into(),
            }),
            "today" => ok(self.today()?),
            "mark_seen" => {
                self.store
                    .set_meta("last_seen", &self.store.now().to_string())?;
                ok(())
            }
            "task" => {
                let a: IdArgs = args(a)?;
                ok(self.task_detail(&a.id)?)
            }
            "complete" => {
                let a: CompleteArgs = args(a)?;
                ok(self.complete(a)?)
            }
            "annotate" => {
                let a: AnnotateArgs = args(a)?;
                self.annotate(a)?;
                ok(())
            }
            "void_completion" => {
                let a: IdArgs = args(a)?;
                let id: CompletionId = parse_id(&a.id, "completion")?;
                let c = self.store.void_completion(id, Some("undone"))?;
                ok(self.card_by_id(c.task)?)
            }
            "snooze_options" => ok(self.snooze_options()?),
            "snooze" => {
                let a: SnoozeArgs = args(a)?;
                let t = self.task(&a.id)?;
                self.store.snooze_task(t.id, a.until, a.reason.as_deref())?;
                ok(self.card_by_id(t.id)?)
            }
            "review" => {
                let a: ReviewArgs = args(a)?;
                ok(self.review(a)?)
            }
            "set_active" => {
                let a: SetActiveArgs = args(a)?;
                let mut t = self.task(&a.id)?;
                t.active = a.active;
                if a.active {
                    t.paused = false;
                    t.not_applicable = false;
                }
                self.store.put(&t)?;
                ok(self.card_by_id(t.id)?)
            }
            "items" => ok(self.items()?),
            "item" => {
                let a: IdArgs = args(a)?;
                ok(self.item_detail(&a.id)?)
            }
            "rooms" => ok(self.rooms()?),
            "add_room" => {
                let a: NameArgs = args(a)?;
                ok(self.add_room(&a.name)?)
            }
            "members" => ok(self.members()?),
            "templates" => ok(self.templates()?),
            "add_from_template" => {
                let a: AddFromTemplateArgs = args(a)?;
                ok(self.add_from_template(a)?)
            }
            "start_small" => {
                let a: StartSmallArgs = args(a)?;
                ok(self.start_small(a)?)
            }
            "history" => ok(self.history()?),
            "set_reminder_prefs" => {
                let a: PrefsArgs = args(a)?;
                ok(self.set_prefs(a)?)
            }
            "reminder_plan" => {
                let a: PlanArgs = args(a)?;
                ok(self.reminder_plan(a.horizon_days)?)
            }
            "test_reminder" => {
                let a: TestReminderArgs = args(a)?;
                ok(self.test_reminder(a)?)
            }
            "export" => {
                let e = export::export(&self.store)?;
                ok(ExportOut {
                    filename: format!(
                        "housewright-export-{}.json",
                        self.today_in(&self.household()?.time_zone)?
                    ),
                    json: export::to_json(&e)?,
                })
            }
            "climate_lookup" => {
                let a: ZipArgs = args(a)?;
                ok(climate_out(&a.zip))
            }
            "profile" => ok(self.profile()?),
            "onboard" => {
                let a: OnboardArgs = args(a)?;
                ok(self.onboard(a)?)
            }
            "starting_plan" => ok(self.starting_plan_out(0, None)?),
            "all_suggested" => ok(self.all_suggested()?),
            "answer_suggestion" => {
                let a: AnswerArgs = args(a)?;
                ok(self.answer_suggestion(a)?)
            }
            other => match self.binder_call(other, a.clone()) {
                Some(r) => r,
                None => self.records_call(other, a),
            },
        }
    }

    /// Like [`Api::call`] with JSON text in and out; what the wasm and JNI
    /// bridges use. Errors come back as `{"error": {...}}`.
    pub fn call_json(&mut self, cmd: &str, args_json: &str) -> String {
        let parsed = if args_json.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str(args_json)
                .map_err(|e| ApiError::invalid(format!("arguments are not JSON: {e}")))
        };
        let result = parsed.and_then(|a| self.call(cmd, a));
        let out = match result {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "error": e }),
        };
        out.to_string()
    }

    // -- helpers ----------------------------------------------------------

    fn household(&self) -> Result<Household, ApiError> {
        Ok(self.store.household()?)
    }

    fn me(&self) -> Result<Member, ApiError> {
        let id = self.store.author()?;
        Ok(self.store.fetch(&id.to_string())?)
    }

    fn today_in(&self, tz: &str) -> Result<Date, ApiError> {
        let tz = TimeZone::get(tz)?;
        Ok(housewright_core::schedule::civil_date(
            self.store.now(),
            &tz,
        ))
    }

    fn task(&self, id: &str) -> Result<Task, ApiError> {
        let id: TaskId = parse_id(id, "task")?;
        Ok(self.store.fetch(&id.to_string())?)
    }

    fn first_property(&self) -> Result<Property, ApiError> {
        self.store
            .list::<Property>()?
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::invalid("no property yet".to_string()))
    }

    fn names(&self) -> Result<Names, ApiError> {
        let items = self
            .store
            .list::<Item>()?
            .into_iter()
            .map(|i| (i.id, i))
            .collect();
        let rooms = self
            .store
            .list::<Room>()?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();
        let members = self
            .store
            .list::<Member>()?
            .into_iter()
            .map(|m| (m.id, m.name))
            .collect();
        Ok(Names {
            items,
            rooms,
            members,
        })
    }

    fn card(&self, t: &Task, s: &TaskStatus, names: &Names) -> Card {
        let item = t.item.and_then(|i| names.items.get(&i));
        let room = t.room.or(item.and_then(|i| i.room));
        Card {
            id: t.id.to_string(),
            title: t.title.clone(),
            item_id: t.item.map(|i| i.to_string()),
            item_name: item.map(|i| i.name.clone()),
            room_name: room.and_then(|r| names.rooms.get(&r).cloned()),
            state: s.eval.state,
            state_label: s.eval.state.label().into(),
            priority: t.priority,
            priority_label: t.priority.label().into(),
            due: s.eval.window.map(|w| w.due),
            earliest: s.eval.window.map(|w| w.earliest),
            overdue_after: s.eval.window.map(|w| w.overdue_after),
            last_done: s.eval.last_done,
            snooze_count: s.eval.snooze_count,
            needs_review: s.eval.needs_review,
            minutes: t.minutes,
            owner_id: t.owner.to_string(),
            owner_name: names
                .members
                .get(&t.owner)
                .cloned()
                .unwrap_or_else(|| "someone".into()),
            active: t.active,
            schedule_text: t.schedule.describe(),
        }
    }

    fn card_by_id(&self, id: TaskId) -> Result<Card, ApiError> {
        let t: Task = self.store.fetch(&id.to_string())?;
        let s = self.store.task_status(&t, self.store.now())?;
        Ok(self.card(&t, &s, &self.names()?))
    }

    fn member_dto(m: &Member) -> MemberDto {
        MemberDto {
            id: m.id.to_string(),
            name: m.name.clone(),
            role: m.role,
            reminders: m.reminders.clone().unwrap_or_default(),
        }
    }

    // -- commands ---------------------------------------------------------

    fn status(&self) -> Result<StatusOut, ApiError> {
        if self.store.meta("household_id")?.is_none() {
            return Ok(StatusOut {
                initialized: false,
                household_name: None,
                me: None,
                property: None,
                currency: None,
            });
        }
        let h = self.household()?;
        let p = self.store.list::<Property>()?.into_iter().next();
        Ok(StatusOut {
            initialized: true,
            currency: Some(h.currency().to_string()),
            household_name: Some(h.name),
            me: Some(Self::member_dto(&self.me()?)),
            property: p.map(|p| PropertyDto {
                id: p.id.to_string(),
                name: p.name,
                time_zone: p.time_zone,
            }),
        })
    }

    fn init(&mut self, a: InitArgs) -> Result<StatusOut, ApiError> {
        let need = |s: &str, what: &str| {
            if s.trim().is_empty() {
                Err(ApiError::invalid(format!("{what} is needed")))
            } else {
                Ok(())
            }
        };
        need(&a.your_name, "your name")?;
        need(&a.property_name, "a name for the house")?;
        let household = if a.household_name.trim().is_empty() {
            format!("{}'s household", a.your_name.trim())
        } else {
            a.household_name.trim().to_string()
        };
        self.store
            .init_household(&household, a.your_name.trim(), &a.time_zone)?;
        let p = Property {
            id: PropertyId::new(),
            name: a.property_name.trim().to_string(),
            address: None,
            time_zone: a.time_zone.clone(),
            climate: Climate::default(),
        };
        self.store.put(&p)?;
        self.store
            .set_meta("last_seen", &self.store.now().to_string())?;
        self.status()
    }

    fn import(&mut self, text: &str) -> Result<StatusOut, ApiError> {
        let e = export::from_json(text)?;
        export::import(&mut self.store, &e, None)?;
        self.status()
    }

    fn today(&mut self) -> Result<TodayOut, ApiError> {
        let me = self.store.author()?;
        let now = self.store.now();
        let ranked = self.store.ranked_tasks(me, now, true)?;
        let names = self.names()?;
        let statuses: Vec<TaskStatus> = ranked
            .iter()
            .filter(|(t, _)| t.active)
            .map(|(_, s)| s.clone())
            .collect();
        let last_seen = self
            .store
            .meta("last_seen")?
            .and_then(|s| s.parse::<Timestamp>().ok());
        let lapsed = priority::is_lapsed(last_seen, now, &statuses);
        if !lapsed {
            self.store.set_meta("last_seen", &now.to_string())?;
        }
        let welcome_back = if lapsed {
            let wb = priority::welcome_back(&statuses);
            let focus = wb
                .focus
                .iter()
                .filter_map(|s| ranked.iter().find(|(t, _)| t.id == s.task))
                .map(|(t, s)| self.card(t, s, &names))
                .collect();
            Some(WelcomeBackOut {
                focus,
                others_overdue: wb.others_overdue,
            })
        } else {
            None
        };
        let mut tasks = Vec::new();
        let mut coming_up = Vec::new();
        let mut active = 0;
        let mut suggested = 0;
        for (t, s) in &ranked {
            if !t.active {
                if s.eval.state.is_live() {
                    suggested += 1;
                }
                continue;
            }
            active += 1;
            match s.eval.state {
                TaskState::DueSoon | TaskState::Due | TaskState::Overdue => {
                    tasks.push(self.card(t, s, &names))
                }
                TaskState::Scheduled | TaskState::Snoozed => {
                    coming_up.push(self.card(t, s, &names))
                }
                _ => {}
            }
        }
        coming_up.sort_by_key(|a| a.due);
        coming_up.truncate(5);
        let p = self.first_property()?;
        let suggestion = self.store.weekly_suggestion(p.id, now)?;
        let has_profile = self.store.profile(p.id)?.is_some();
        Ok(TodayOut {
            suggestion,
            has_profile,
            tasks,
            coming_up,
            lapsed,
            welcome_back,
            active_count: active,
            suggested_count: suggested,
        })
    }

    fn task_detail(&self, id: &str) -> Result<TaskDetail, ApiError> {
        let t = self.task(id)?;
        let s = self.store.task_status(&t, self.store.now())?;
        let names = self.names()?;
        let completions = self.completions_of(&t, &names)?;
        let interval = match &t.schedule {
            Schedule::Fixed { every, .. } | Schedule::Floating { every } => Some(every.to_string()),
            _ => None,
        };
        Ok(TaskDetail {
            card: self.card(&t, &s, &names),
            steps: t.steps.clone(),
            why: t.why.as_ref().map(|w| w.text.clone()),
            source: t.why.as_ref().map(|w| w.source.clone()),
            interval,
            paused: t.paused,
            not_applicable: t.not_applicable,
            completions,
            completion_fields: t.completion_fields.clone(),
        })
    }

    fn completions_of(&self, t: &Task, names: &Names) -> Result<Vec<CompletionOut>, ApiError> {
        let all: Vec<Completion> = self.store.list_by("task", &t.id.to_string())?;
        let voided: Vec<CompletionId> = all.iter().filter_map(|c| c.voids).collect();
        let mut out = Vec::new();
        let all_notes: Vec<Note> = self.store.list()?;
        for c in all.iter().filter(|c| c.voids.is_none()) {
            let cid = c.id.to_string();
            let notes = all_notes
                .iter()
                .filter(|n| n.about.kind == EntityKind::Completion && n.about.id == cid)
                .cloned();
            let mut texts: Vec<String> = c.notes.iter().map(|n| n.value.clone()).collect();
            texts.extend(notes.into_iter().map(|n| n.text.value));
            let (attachments, costs, values) = self.completion_extras(c, t)?;
            let cost_total = (!costs.is_empty()).then(|| costs.iter().map(|l| l.minor).sum());
            out.push(CompletionOut {
                attachments,
                costs,
                cost_total,
                values,
                id: c.id.to_string(),
                task_id: t.id.to_string(),
                task_title: t.title.clone(),
                at: c.at,
                by_name: names
                    .members
                    .get(&c.by)
                    .cloned()
                    .unwrap_or_else(|| "someone".into()),
                notes: texts,
                voided: voided.contains(&c.id),
            });
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.at));
        Ok(out)
    }

    fn complete(&mut self, a: CompleteArgs) -> Result<CompleteOut, ApiError> {
        let t = self.task(&a.id)?;
        let at = match a.at_ms {
            Some(ms) => Some(Timestamp::from_millisecond(ms)?),
            None => None,
        };
        let notes = a.notes.filter(|n| !n.trim().is_empty());
        let c = self.store.complete_task(t.id, at, notes.as_deref())?;
        self.store
            .set_meta("last_seen", &self.store.now().to_string())?;
        Ok(CompleteOut {
            completion_id: c.id.to_string(),
            card: self.card_by_id(t.id)?,
        })
    }

    fn annotate(&mut self, a: AnnotateArgs) -> Result<(), ApiError> {
        let id: CompletionId = parse_id(&a.completion_id, "completion")?;
        let _: Completion = self.store.fetch(&id.to_string())?;
        if a.text.trim().is_empty() {
            return Ok(());
        }
        let n = Note {
            id: NoteId::new(),
            about: EntityRef {
                kind: EntityKind::Completion,
                id: id.to_string(),
            },
            by: self.store.author()?,
            at: self.store.now(),
            text: Flagged::normal(a.text.trim().to_string()),
        };
        self.store.append(&n)?;
        Ok(())
    }

    fn snooze_options(&self) -> Result<Vec<SnoozeOption>, ApiError> {
        let p = self.first_property()?;
        let today = self.today_in(&p.time_zone)?;
        let wd = today.weekday();
        // Days until the next Saturday; on a weekend, the following one.
        let to_sat = (Weekday::Saturday.to_monday_one_offset() - wd.to_monday_one_offset())
            .rem_euclid(7) as i64;
        let (weekend_label, to_sat) = match wd {
            Weekday::Saturday | Weekday::Sunday => {
                ("Next weekend", if to_sat == 0 { 7 } else { to_sat })
            }
            _ => ("This weekend", to_sat),
        };
        let to_mon = (8 - wd.to_monday_one_offset() as i64).rem_euclid(7);
        let to_mon = if to_mon == 0 { 7 } else { to_mon };
        Ok(vec![
            SnoozeOption {
                key: "tomorrow".into(),
                label: "Tomorrow".into(),
                date: add_days(today, 1),
            },
            SnoozeOption {
                key: "weekend".into(),
                label: weekend_label.into(),
                date: add_days(today, to_sat),
            },
            SnoozeOption {
                key: "next_week".into(),
                label: "Next week".into(),
                date: add_days(today, to_mon),
            },
            SnoozeOption {
                key: "next_month".into(),
                label: "In a month".into(),
                date: today.checked_add(Span::new().months(1)).unwrap_or(today),
            },
        ])
    }

    fn review(&mut self, a: ReviewArgs) -> Result<Card, ApiError> {
        let mut t = self.task(&a.id)?;
        match a.action.as_str() {
            "change_interval" => {
                let every: Interval = a
                    .every
                    .as_deref()
                    .ok_or_else(|| ApiError::invalid("say how often".to_string()))?
                    .parse()
                    .map_err(ApiError::invalid)?;
                t.schedule = match t.schedule {
                    Schedule::Fixed { anchor, .. } => Schedule::Fixed { every, anchor },
                    Schedule::Floating { .. } => Schedule::Floating { every },
                    // Other kinds become "N after last done".
                    _ => Schedule::Floating { every },
                };
                t.window = None;
            }
            "reassign" => {
                let m: MemberId = parse_id(
                    a.member
                        .as_deref()
                        .ok_or_else(|| ApiError::invalid("say who".to_string()))?,
                    "member",
                )?;
                let _: Member = self.store.fetch(&m.to_string())?;
                t.owner = m;
            }
            "not_applicable" => t.not_applicable = true,
            "pause" => t.paused = true,
            "resume" => {
                t.paused = false;
                t.not_applicable = false;
            }
            other => {
                return Err(ApiError::invalid(format!(
                    "unknown review action '{other}'"
                )));
            }
        }
        self.store.put(&t)?;
        self.card_by_id(t.id)
    }

    fn items(&self) -> Result<Vec<ItemRow>, ApiError> {
        let names = self.names()?;
        let now = self.store.now();
        let tasks: Vec<Task> = self.store.list()?;
        let mut rows = Vec::new();
        for i in names.items.values() {
            let mut next: Option<(TaskState, Option<Date>)> = None;
            for t in tasks.iter().filter(|t| t.item == Some(i.id) && t.active) {
                let s = self.store.task_status(t, now)?;
                if !s.eval.state.is_live() {
                    continue;
                }
                let cand = (s.eval.state, s.eval.window.map(|w| w.due));
                next = match next {
                    None => Some(cand),
                    Some(cur) => {
                        let better = cand.0.urgency() > cur.0.urgency()
                            || (cand.0.urgency() == cur.0.urgency() && cand.1 < cur.1);
                        Some(if better { cand } else { cur })
                    }
                };
            }
            rows.push(ItemRow {
                id: i.id.to_string(),
                name: i.name.clone(),
                category: i.category.clone(),
                room_id: i.room.map(|r| r.to_string()),
                room_name: i.room.and_then(|r| names.rooms.get(&r).cloned()),
                template: i.template.clone(),
                state: next.map(|n| n.0),
                next_due: next.and_then(|n| n.1),
            });
        }
        rows.sort_by_key(|a| a.name.to_lowercase());
        Ok(rows)
    }

    fn item_detail(&self, id: &str) -> Result<ItemDetail, ApiError> {
        let id: ItemId = parse_id(id, "item")?;
        let item: Item = self.store.fetch(&id.to_string())?;
        let names = self.names()?;
        let now = self.store.now();
        let row = self
            .items()?
            .into_iter()
            .find(|r| r.id == id.to_string())
            .ok_or_else(|| ApiError::internal("item vanished"))?;
        let template = match &item.template {
            Some(t) => self.store.get::<housewright_core::template::Template>(t)?,
            None => None,
        };
        let defs: Vec<_> = template
            .iter()
            .flat_map(|t| t.fields.iter())
            .chain(item.extra_fields.iter())
            .collect();
        let mut fields = Vec::new();
        for (key, v) in &item.custom {
            let label = defs
                .iter()
                .find(|d| &d.key == key)
                .map(|d| d.label.clone())
                .unwrap_or_else(|| key.clone());
            let sensitive = v.sensitivity == Sensitivity::Sensitive;
            fields.push(FieldOut {
                key: key.clone(),
                label,
                // Sensitive values stay on the device's database until the
                // person taps to reveal them (`reveal_field`).
                value: if sensitive {
                    String::new()
                } else {
                    value_text(&v.value)
                },
                sensitive,
            });
        }
        let mut tasks = Vec::new();
        let mut history = Vec::new();
        let mut item_tasks: Vec<Task> = self.store.list_by("item", &id.to_string())?;
        item_tasks.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.title.cmp(&b.title)));
        for t in &item_tasks {
            let s = self.store.task_status(t, now)?;
            tasks.push(self.card(t, &s, &names));
            history.extend(self.completions_of(t, &names)?);
        }
        history.sort_by_key(|a| std::cmp::Reverse(a.at));
        let next = tasks
            .iter()
            .filter(|c| c.active && c.state.is_live())
            .max_by(|a, b| {
                a.state
                    .urgency()
                    .cmp(&b.state.urgency())
                    .then(b.due.cmp(&a.due))
            })
            .cloned();
        let attachments = self
            .store
            .attachments_about(&EntityRef {
                kind: EntityKind::Item,
                id: id.to_string(),
            })?
            .iter()
            .map(records::attachment_out)
            .collect();
        let edit = self.field_edits(&item)?;
        let serial_sensitive = item
            .serial
            .as_ref()
            .is_some_and(|s| s.sensitivity == Sensitivity::Sensitive);
        Ok(ItemDetail {
            row,
            make: item.make,
            model: item.model,
            serial: item
                .serial
                .filter(|s| s.sensitivity == Sensitivity::Normal)
                .map(|s| s.value),
            installed: item.installed,
            purchased: item.purchased,
            fields,
            tasks,
            history,
            code: housewright_core::labels::short_code(id),
            next,
            attachments,
            edit,
            serial_sensitive,
        })
    }

    fn rooms(&self) -> Result<Vec<RoomDto>, ApiError> {
        let mut r: Vec<RoomDto> = self
            .store
            .list::<Room>()?
            .into_iter()
            .map(|r| RoomDto {
                id: r.id.to_string(),
                name: r.name,
            })
            .collect();
        r.sort_by_key(|a| a.name.to_lowercase());
        Ok(r)
    }

    fn add_room(&mut self, name: &str) -> Result<RoomDto, ApiError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::invalid("give the room a name".to_string()));
        }
        if let Some(r) = self
            .store
            .list::<Room>()?
            .into_iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
        {
            return Ok(RoomDto {
                id: r.id.to_string(),
                name: r.name,
            });
        }
        let p = self.first_property()?;
        let r = Room {
            id: RoomId::new(),
            property: p.id,
            floor: None,
            name: name.to_string(),
        };
        self.store.put(&r)?;
        Ok(RoomDto {
            id: r.id.to_string(),
            name: r.name,
        })
    }

    fn members(&self) -> Result<Vec<MemberDto>, ApiError> {
        Ok(self
            .store
            .list::<Member>()?
            .iter()
            .map(Self::member_dto)
            .collect())
    }

    fn templates(&self) -> Result<Vec<TemplateSummary>, ApiError> {
        let mut out = Vec::new();
        for id in self.library.ids() {
            let t = self.library.resolve(id)?;
            out.push(TemplateSummary {
                id: t.id.clone(),
                name: t.name.clone(),
                category: t.category.clone(),
                description: t.description.clone(),
                fields: t.fields.clone(),
                tasks: t
                    .tasks
                    .iter()
                    .map(|k| TemplateTaskSummary {
                        key: k.key.clone(),
                        title: k.title.clone(),
                        priority: k.priority,
                        schedule_text: k.schedule.describe(),
                    })
                    .collect(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn add_from_template(&mut self, a: AddFromTemplateArgs) -> Result<AddedItem, ApiError> {
        let t = self.library.resolve(&a.template)?;
        let p = self.first_property()?;
        let room = match (a.room_id.as_deref(), a.new_room.as_deref()) {
            (Some(r), _) if !r.is_empty() => {
                let id: RoomId = parse_id(r, "room")?;
                let _: Room = self.store.fetch(&id.to_string())?;
                Some(id)
            }
            (_, Some(n)) if !n.trim().is_empty() => Some(parse_id(&self.add_room(n)?.id, "room")?),
            _ => None,
        };
        let name = a
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| t.name.clone());
        let mut custom = BTreeMap::new();
        for (k, raw) in a.values {
            if raw.trim().is_empty() {
                continue;
            }
            let def = t
                .fields
                .iter()
                .find(|f| f.key == k)
                .ok_or_else(|| ApiError::invalid(format!("no field '{k}' on this template")))?;
            let value = parse_value(&def.field_type, raw.trim())?;
            def.check(&value).map_err(ApiError::invalid)?;
            custom.insert(
                k,
                CustomValue {
                    value,
                    sensitivity: def.sensitivity,
                },
            );
        }
        let none_if_blank = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
        let item = Item {
            id: ItemId::new(),
            property: p.id,
            room,
            parent: None,
            name: name.trim().to_string(),
            category: t.category.clone(),
            make: none_if_blank(a.make),
            model: none_if_blank(a.model),
            serial: none_if_blank(a.serial).map(|s| Flagged::normal(s.trim().to_string())),
            purchased: None,
            installed: a.installed,
            warranty: None,
            location_note: None,
            template: Some(t.id.clone()),
            extra_fields: vec![],
            custom,
            sensitivity: Sensitivity::Normal,
        };
        let me = self.store.author()?;
        let start = self.today_in(&p.time_zone)?;
        let tasks = self
            .store
            .batch(|s| s.add_item_from_template(&item, &t, me, start))?;
        let names = self.names()?;
        let now = self.store.now();
        let mut cards = Vec::new();
        for task in &tasks {
            let s = self.store.task_status(task, now)?;
            cards.push(self.card(task, &s, &names));
        }
        Ok(AddedItem {
            item_id: item.id.to_string(),
            tasks: cards,
        })
    }

    /// P3: switch on the top suggested tasks, for one item or the whole house.
    fn start_small(&mut self, a: StartSmallArgs) -> Result<Vec<Card>, ApiError> {
        let n = a.n.unwrap_or(START_SMALL_DEFAULT);
        let now = self.store.now();
        let picked = match a.item_id {
            None => self.store.start_small(n, now)?,
            Some(item) => {
                let item: ItemId = parse_id(&item, "item")?;
                let tasks: Vec<Task> = self.store.list_by("item", &item.to_string())?;
                let mut statuses = Vec::new();
                for t in tasks.iter().filter(|t| !t.active) {
                    statuses.push(self.store.task_status(t, now)?);
                }
                let picked = priority::start_small(&statuses, n);
                for id in &picked {
                    let mut t: Task = self.store.fetch(&id.to_string())?;
                    t.active = true;
                    self.store.put(&t)?;
                }
                picked
            }
        };
        picked.into_iter().map(|id| self.card_by_id(id)).collect()
    }

    fn history(&self) -> Result<Vec<HistoryEntry>, ApiError> {
        let h = self.household()?;
        let tz = TimeZone::get(&h.time_zone)?;
        let names = self.names()?;
        let mut out: Vec<HistoryEntry> = audit_log(&self.store, &tz)?
            .into_iter()
            .map(|e| HistoryEntry {
                at: e.at,
                author: names
                    .members
                    .get(&e.author)
                    .cloned()
                    .unwrap_or_else(|| "someone".into()),
                text: e.text,
            })
            .collect();
        out.reverse();
        Ok(out)
    }

    fn set_prefs(&mut self, a: PrefsArgs) -> Result<MemberDto, ApiError> {
        let id: MemberId = match a.member_id {
            Some(m) => parse_id(&m, "member")?,
            None => self.store.author()?,
        };
        let mut m: Member = self.store.fetch(&id.to_string())?;
        m.reminders = if a.prefs == ReminderPrefs::default() {
            None
        } else {
            Some(a.prefs)
        };
        self.store.put(&m)?;
        Ok(Self::member_dto(&m))
    }

    /// Reminders this device should schedule for its own member.
    pub fn reminder_plan(&self, horizon_days: Option<i64>) -> Result<Vec<ReminderOut>, ApiError> {
        if self.store.meta("household_id")?.is_none() {
            return Ok(vec![]);
        }
        let me = self.me()?;
        let now = self.store.now();
        let names = self.names()?;
        let mut by_tz: BTreeMap<String, Vec<(Task, TaskStatus)>> = BTreeMap::new();
        let props: BTreeMap<PropertyId, Property> = self
            .store
            .list::<Property>()?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        for (t, s) in self.store.ranked_tasks(me.id, now, false)? {
            // Reminders go to the task's owner (P8).
            if t.owner != me.id {
                continue;
            }
            let tz = props
                .get(&t.property)
                .map(|p| p.time_zone.clone())
                .unwrap_or_else(|| "UTC".into());
            by_tz.entry(tz).or_default().push((t, s));
        }
        let prefs = me.reminders.clone().unwrap_or_default();
        let mut all = Vec::new();
        for (tz, tasks) in by_tz {
            let tz = TimeZone::get(&tz)?;
            let statuses: Vec<TaskStatus> = tasks.iter().map(|(_, s)| s.clone()).collect();
            let texts: Vec<ReminderText> = tasks
                .iter()
                .map(|(t, _)| reminder_text(t, &names))
                .collect();
            all.extend(reminders::plan(&PlanInput {
                statuses: &statuses,
                texts: &texts,
                prefs: &prefs,
                now,
                tz: &tz,
                horizon_days: horizon_days.unwrap_or(reminders::DEFAULT_HORIZON_DAYS),
            }));
        }
        all.sort_by_key(|a| a.at);
        all.truncate(reminders::MAX_PLANNED);
        Ok(all.into_iter().map(ReminderOut::from).collect())
    }

    /// A reminder for a real task a few seconds from now, so people (and the
    /// Phase 2 proof) can check that notifications and their buttons work.
    fn test_reminder(&self, a: TestReminderArgs) -> Result<ReminderOut, ApiError> {
        let me = self.store.author()?;
        let now = self.store.now();
        let task = match a.task_id {
            Some(id) => self.task(&id)?,
            None => self
                .store
                .ranked_tasks(me, now, false)?
                .into_iter()
                .find(|(_, s)| s.eval.state.is_live())
                .map(|(t, _)| t)
                .ok_or_else(|| {
                    ApiError::invalid("switch on a task first, then try a reminder".to_string())
                })?,
        };
        let names = self.names()?;
        let text = reminder_text(&task, &names);
        let mut body = String::from("Test reminder.");
        if let Some(i) = text.item {
            body.push_str(&format!(" {i}."));
        }
        if let Some(w) = text.why {
            body.push(' ');
            body.push_str(&w);
        }
        let secs = a.in_seconds.unwrap_or(60).clamp(5, 3600);
        Ok(ReminderOut {
            task_id: task.id.to_string(),
            kind: reminders::ReminderKind::Due,
            at_ms: now.as_millisecond() + secs * 1000,
            title: task.title.clone(),
            body,
        })
    }
}

impl Api {
    fn profile(&self) -> Result<ProfileOut, ApiError> {
        let p = self.first_property()?;
        let profile = self.store.profile(p.id)?;
        let climate = profile
            .as_ref()
            .and_then(|x| x.zip.as_ref())
            .and_then(|z| climate_out(&z.value));
        Ok(ProfileOut { profile, climate })
    }

    /// Save the answers and add what they imply. The first time (nothing
    /// switched on yet) a small starting plan is switched on; answering
    /// again only adds things and suggestions (D-65).
    fn onboard(&mut self, a: OnboardArgs) -> Result<StartingPlanOut, ApiError> {
        let p = self.first_property()?;
        let mut answers = a.profile.answers;
        answers.insert("id".into(), json!(p.id.to_string()));
        if let Some(z) = answers.get("zip").cloned() {
            // Accept a bare string for convenience; store it flagged (D-41).
            if let Some(text) = z.as_str() {
                let text = text.trim();
                if text.is_empty() {
                    answers.remove("zip");
                } else {
                    if housewright_core::climate::parse_zip(text).is_none() {
                        return Err(ApiError::invalid(format!(
                            "'{text}' is not a five-digit ZIP code"
                        )));
                    }
                    answers.insert(
                        "zip".into(),
                        json!({"value": &text[..5], "sensitivity": "normal"}),
                    );
                }
            }
        }
        let profile: Profile = serde_json::from_value(Value::Object(answers))
            .map_err(|e| ApiError::invalid(format!("bad answers: {e}")))?;
        let first_time = !self
            .store
            .list::<Task>()?
            .iter()
            .any(|t| t.property == p.id && t.active);
        let me = self.store.author()?;
        let start = self.today_in(&p.time_zone)?;
        let now = self.store.now();
        let library = &self.library;
        // One transaction: twenty things and their tasks are hundreds of
        // changes, which is slow one by one on the web's storage.
        let (out, picked) = self.store.batch(|s| {
            let out = s.apply_profile(&profile, library, me, start)?;
            let picked = if first_time {
                s.switch_on_starting_plan(p.id, starting_count(&profile), now)?
            } else {
                vec![]
            };
            Ok((out, picked))
        })?;
        self.starting_plan_out(out.new_items.len(), Some(picked))
    }

    /// The switched-on tasks with their reasons. `order` puts the given
    /// tasks first in that order (the plan just chosen).
    fn starting_plan_out(
        &self,
        new_things: usize,
        order: Option<Vec<TaskId>>,
    ) -> Result<StartingPlanOut, ApiError> {
        let p = self.first_property()?;
        let me = self.store.author()?;
        let now = self.store.now();
        let names = self.names()?;
        let ranked = self.store.ranked_tasks(me, now, true)?;
        let mut on: Vec<(&Task, &TaskStatus)> = ranked
            .iter()
            .filter(|(t, _)| t.active && t.property == p.id)
            .map(|(t, s)| (t, s))
            .collect();
        if let Some(order) = &order {
            on.sort_by_key(|(t, _)| order.iter().position(|x| *x == t.id).unwrap_or(usize::MAX));
        }
        let switched_on = on
            .iter()
            .map(|(t, s)| self.plan_task(t, s, &names))
            .collect();
        let suggested_count = ranked
            .iter()
            .filter(|(t, s)| !t.active && t.property == p.id && s.eval.state.is_live())
            .count();
        let things_recorded = names.items.values().filter(|i| i.property == p.id).count();
        let climate = self
            .store
            .profile(p.id)?
            .and_then(|x| x.zip)
            .and_then(|z| climate_out(&z.value));
        Ok(StartingPlanOut {
            switched_on,
            things_recorded,
            new_things,
            suggested_count,
            climate,
        })
    }

    fn plan_task(&self, t: &Task, s: &TaskStatus, names: &Names) -> PlanTask {
        PlanTask {
            card: self.card(t, s, names),
            why: t.why.as_ref().map(|w| w.text.clone()),
            source: t.why.as_ref().map(|w| w.source.clone()),
        }
    }

    /// "See everything we suggested": switched-off tasks, by thing.
    fn all_suggested(&self) -> Result<Vec<SuggestedGroup>, ApiError> {
        let me = self.store.author()?;
        let now = self.store.now();
        let names = self.names()?;
        let mut groups: Vec<SuggestedGroup> = Vec::new();
        for (t, s) in self.store.ranked_tasks(me, now, true)? {
            if t.active || !s.eval.state.is_live() {
                continue;
            }
            let item = t.item.and_then(|i| names.items.get(&i));
            let key = t.item.map(|i| i.to_string());
            let pt = self.plan_task(&t, &s, &names);
            match groups.iter_mut().find(|g| g.item_id == key) {
                Some(g) => g.tasks.push(pt),
                None => groups.push(SuggestedGroup {
                    item_id: key,
                    item_name: item
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| "Other".into()),
                    category: item.and_then(|i| i.category.clone()),
                    tasks: vec![pt],
                }),
            }
        }
        Ok(groups)
    }

    fn answer_suggestion(&mut self, a: AnswerArgs) -> Result<AnswerOut, ApiError> {
        let p = self.first_property()?;
        let me = self.store.author()?;
        let now = self.store.now();
        let library = &self.library;
        let ids = self
            .store
            .batch(|s| s.answer_suggestion(p.id, &a.key, a.answer, library, me, now))?;
        let switched_on = ids
            .into_iter()
            .map(|id| self.card_by_id(id))
            .collect::<Result<_, _>>()?;
        Ok(AnswerOut { switched_on })
    }
}

/// Look up a ZIP code and write the dates out for people.
fn climate_out(zip: &str) -> Option<ClimateOut> {
    let lookup = housewright_core::climate::lookup_us_zip(zip)?;
    let day = |doy: Option<u16>| {
        let d = Date::new(2025, 1, 1)
            .ok()?
            .checked_add(Span::new().days(i64::from(doy?) - 1))
            .ok()?;
        Some(d.strftime("%b %-d").to_string())
    };
    let freezes = lookup.climate.freezes();
    Some(ClimateOut {
        first_frost: day(lookup.climate.first_frost_doy),
        last_frost: day(lookup.climate.last_frost_doy),
        freezes,
        lookup,
    })
}

struct Names {
    items: BTreeMap<ItemId, Item>,
    rooms: BTreeMap<RoomId, String>,
    members: BTreeMap<MemberId, String>,
}

fn reminder_text(t: &Task, names: &Names) -> ReminderText {
    let item = t.item.and_then(|i| names.items.get(&i));
    ReminderText {
        task: t.id,
        item: item.map(|i| i.name.clone()),
        place: t
            .room
            .or(item.and_then(|i| i.room))
            .and_then(|r| names.rooms.get(&r).cloned()),
        why: t.why.as_ref().map(|w| w.text.clone()),
        minutes: t.minutes,
    }
}

fn parse_value(t: &FieldType, raw: &str) -> Result<FieldValue, ApiError> {
    Ok(match t {
        FieldType::Text => FieldValue::Text(raw.into()),
        FieldType::Number { unit } => FieldValue::Number {
            value: raw
                .parse()
                .map_err(|_| ApiError::invalid(format!("'{raw}' is not a number")))?,
            unit: unit.clone(),
        },
        FieldType::Date => FieldValue::Date(
            raw.parse()
                .map_err(|e| ApiError::invalid(format!("'{raw}' is not a date: {e}")))?,
        ),
        FieldType::YesNo => FieldValue::YesNo(match raw {
            "yes" | "true" => true,
            "no" | "false" => false,
            _ => return Err(ApiError::invalid(format!("'{raw}' should be yes or no"))),
        }),
        FieldType::Choice { .. } => FieldValue::Choice(raw.into()),
        FieldType::Photo => FieldValue::Photo(parse_id(raw, "photo")?),
    })
}

fn value_text(v: &FieldValue) -> String {
    match v {
        FieldValue::Text(s) | FieldValue::Choice(s) => s.clone(),
        FieldValue::Number { value, unit } => match unit {
            Some(u) => format!("{value} {u}"),
            None => value.to_string(),
        },
        FieldValue::Date(d) => d.to_string(),
        FieldValue::YesNo(b) => if *b { "yes" } else { "no" }.into(),
        FieldValue::Photo(_) => "photo".into(),
    }
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}
