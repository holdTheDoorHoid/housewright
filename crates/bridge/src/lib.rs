//! The Housewright bridge for Home Assistant (D-117).
//!
//! A headless Housewright device. It joins a household with the normal
//! pairing flow (code and safety number), keeps its own database and keys,
//! syncs with the household's server like any other device, and offers a
//! small JSON API on this computer (127.0.0.1 by default) for the Home
//! Assistant integration in `integrations/home-assistant/`.
//!
//! Every change goes through the core and the operation log, so it syncs
//! like a change made in the app. What leaves through the API is built
//! field by field in [`dto`]; nothing sensitive (D-41) is ever copied into
//! it: no serial numbers, custom fields, notes, costs, contacts, facts or
//! locations, and nothing about items marked sensitive.
//!
//! Licensed AGPL-3.0-only.

pub mod dto;
pub mod http;
pub mod matching;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use housewright_api::{Api, ApiError};
use housewright_core::blobs::DirBlobs;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::schedule::{Schedule, TaskState, civil_date, valid_event_name};
use housewright_core::store::{Store, SystemClock, WallClock};
use housewright_crypto::roster::DeviceRole;
use housewright_sync::native::UreqTransport;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde_json::{Value, json};

pub use dto::*;

/// What the bridge calls itself when it joins.
pub const DEFAULT_DEVICE_NAME: &str = "Home Assistant";

/// Readings sent with `throttle` are stored at most this often per meter
/// (D-121): Home Assistant sensors can change every few seconds, and every
/// stored reading is synced to every device for good.
pub const THROTTLE: Duration = Duration::from_secs(30 * 60);

/// The same event on the same item within this time is one event.
pub const EVENT_DEDUPE: Duration = Duration::from_secs(10 * 60);

/// Requests to the sync server give up after this long, so Home
/// Assistant is not kept waiting by an unreachable server.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(15);

fn not_found(what: &str) -> ApiError {
    ApiError {
        code: "not_found".into(),
        message: what.into(),
    }
}

fn not_paired() -> ApiError {
    ApiError {
        code: "not_paired".into(),
        message: "The bridge has not joined a household yet. Pair it from the Housewright app: Settings, Sync and devices, Add a device or person, Home Assistant.".into(),
    }
}

/// A headless household device.
pub struct Bridge {
    api: Api,
}

impl Bridge {
    /// Open (or create) the bridge's database and files in `dir`, with a
    /// network transport for sync.
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("data folder {}: {e}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The database holds the household's keys (D-95).
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let store = Store::open(dir.join("bridge.db")).map_err(|e| e.to_string())?;
        let blobs = DirBlobs::open(dir.join("files")).map_err(|e| e.to_string())?;
        let mut api = Api::with_blobs(store, Box::new(blobs)).map_err(|e| e.to_string())?;
        api.set_transport(Box::new(UreqTransport::with_timeout(SYNC_TIMEOUT)));
        Ok(Self { api })
    }

    /// In memory (tests). `clock` defaults to the system clock.
    pub fn in_memory(clock: Option<Box<dyn WallClock>>) -> Result<Self, ApiError> {
        let store = Store::open_in_memory(clock.unwrap_or_else(|| Box::new(SystemClock)))?;
        let mut api = Api::new(store)?;
        api.set_transport(Box::new(UreqTransport::with_timeout(SYNC_TIMEOUT)));
        Ok(Self { api })
    }

    pub fn api(&mut self) -> &mut Api {
        &mut self.api
    }

    fn store(&self) -> &Store {
        self.api.store()
    }

    pub fn is_paired(&self) -> bool {
        self.store().meta("household_id").ok().flatten().is_some()
    }

    fn need_household(&self) -> Result<(), ApiError> {
        if self.is_paired() {
            Ok(())
        } else {
            Err(not_paired())
        }
    }

    fn me(&self) -> Result<Member, ApiError> {
        let id = self.store().author()?;
        Ok(self.store().fetch(&id.to_string())?)
    }

    /// Sync now if sync is set up. Errors land in the status.
    pub fn sync_quietly(&mut self) {
        if self.is_paired() {
            self.api.sync_quietly();
        }
    }

    // -- reading ----------------------------------------------------------

    pub fn status(&mut self) -> Result<StatusOut, ApiError> {
        let mut warnings = Vec::new();
        let sync: Value = self.api.call("sync_status", Value::Null)?;
        if !self.is_paired() {
            return Ok(StatusOut {
                service: "housewright-bridge".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                paired: false,
                household: None,
                member: None,
                sync: SyncOut::from_status(&sync),
                warnings,
            });
        }
        let me = self.me()?;
        if me.kind != MemberKind::HomeAssistant {
            warnings.push(
                "This bridge joined as a person, not as Home Assistant, so the app will not label its changes \"via Home Assistant\". Remove it in the app and pair again choosing Home Assistant.".to_string(),
            );
        }
        if me.role == Role::Owner {
            warnings.push(
                "This bridge acts as an owner. It should be a member: remove it in the app and pair again choosing Home Assistant.".to_string(),
            );
        }
        if let Some(w) = sync["warnings"].as_array() {
            warnings.extend(w.iter().filter_map(|x| x.as_str().map(String::from)));
        }
        Ok(StatusOut {
            service: "housewright-bridge".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            paired: true,
            household: Some(self.store().household()?.name),
            member: Some(MemberOut::of(&me)),
            sync: SyncOut::from_status(&sync),
            warnings,
        })
    }

    pub fn members(&self) -> Result<Vec<MemberOut>, ApiError> {
        self.need_household()?;
        let mut v: Vec<MemberOut> = self
            .store()
            .list::<Member>()?
            .iter()
            .map(MemberOut::of)
            .collect();
        v.sort_by_key(|m| m.name.to_lowercase());
        Ok(v)
    }

    fn view(&self) -> Result<View, ApiError> {
        let store = self.store();
        let items: BTreeMap<ItemId, Item> = store
            .list::<Item>()?
            .into_iter()
            .map(|i| (i.id, i))
            .collect();
        let rooms = store
            .list::<Room>()?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();
        let members = store
            .list::<Member>()?
            .into_iter()
            .map(|m| (m.id, m.name))
            .collect();
        let tz = match store.list::<Property>()?.into_iter().next() {
            Some(p) => TimeZone::get(&p.time_zone)?,
            None => TimeZone::UTC,
        };
        Ok(View {
            items,
            rooms,
            members,
            today: civil_date(store.now(), &tz),
        })
    }

    /// Whether a task may be shown: not on an item marked sensitive.
    fn shown(v: &View, t: &Task) -> bool {
        t.item
            .and_then(|i| v.items.get(&i))
            .is_none_or(|i| i.sensitivity == Sensitivity::Normal)
    }

    fn task_out(&self, v: &View, t: &Task) -> Result<TaskOut, ApiError> {
        let s = self.store().task_status(t, self.store().now())?;
        let item = t.item.and_then(|i| v.items.get(&i));
        let room = t.room.or(item.and_then(|i| i.room));
        let w = s.eval.window;
        Ok(TaskOut {
            id: t.id.to_string(),
            title: t.title.clone(),
            state: state_key(s.eval.state).into(),
            state_label: s.eval.state.label().into(),
            active: t.active,
            due: w.map(|w| w.due),
            earliest: w.map(|w| w.earliest),
            overdue_after: w.map(|w| w.overdue_after),
            days_until: w.map(|w| days_between(v.today, w.due)),
            last_done: s.eval.last_done,
            item_id: item.map(|i| i.id.to_string()),
            item_name: item.map(|i| i.name.clone()),
            room_name: room.and_then(|r| v.rooms.get(&r).cloned()),
            owner_id: t.owner.to_string(),
            owner_name: v
                .members
                .get(&t.owner)
                .cloned()
                .unwrap_or_else(|| "someone".into()),
            priority: serde_json::to_value(t.priority)
                .ok()
                .and_then(|p| p.as_str().map(String::from))
                .unwrap_or_default(),
            priority_label: t.priority.label().into(),
            schedule_text: t.schedule.describe(),
            minutes: t.minutes,
            meter: match &t.schedule {
                Schedule::Usage { meter, .. } => Some(meter.clone()),
                _ => None,
            },
            usage_used: s.eval.usage.map(|u| u.used),
            usage_threshold: s.eval.usage.map(|u| u.threshold),
            event: match &t.schedule {
                Schedule::Triggered { event } => Some(event.clone()),
                _ => None,
            },
        })
    }

    /// Switched-on tasks, most urgent first.
    pub fn tasks(&self) -> Result<Vec<TaskOut>, ApiError> {
        self.need_household()?;
        let v = self.view()?;
        let me = self.store().author()?;
        let mut out = Vec::new();
        for (t, _) in self.store().ranked_tasks(me, self.store().now(), false)? {
            if Self::shown(&v, &t) {
                out.push(self.task_out(&v, &t)?);
            }
        }
        Ok(out)
    }

    pub fn items(&self) -> Result<Vec<ItemOut>, ApiError> {
        self.need_household()?;
        let v = self.view()?;
        let tasks: Vec<Task> = self.store().list()?;
        let mut out = Vec::new();
        for i in v.items.values() {
            if i.sensitivity != Sensitivity::Normal {
                continue;
            }
            let mut meters = Vec::new();
            let mut events = Vec::new();
            for m in self.store().list_by::<Meter>("item", &i.id.to_string())? {
                let last = self
                    .store()
                    .readings(m.id)?
                    .into_iter()
                    .max_by_key(|r| r.at);
                let is_event = tasks.iter().any(|t| {
                    t.item == Some(i.id)
                        && matches!(&t.schedule, Schedule::Triggered { event } if *event == m.key)
                });
                if is_event {
                    continue;
                }
                meters.push(MeterOut {
                    key: m.key,
                    name: m.name,
                    unit: m.unit,
                    last_value: last.as_ref().map(|r| r.value),
                    last_at: last.map(|r| r.at),
                });
            }
            for t in tasks.iter().filter(|t| t.item == Some(i.id)) {
                if let Schedule::Triggered { event } = &t.schedule
                    && !events.contains(event)
                {
                    events.push(event.clone());
                }
            }
            out.push(ItemOut {
                id: i.id.to_string(),
                name: i.name.clone(),
                category: i.category.clone(),
                room_name: i.room.and_then(|r| v.rooms.get(&r).cloned()),
                make: i.make.clone(),
                model: i.model.clone(),
                template: i.template.clone(),
                meters,
                events,
            });
        }
        out.sort_by_key(|i| i.name.to_lowercase());
        Ok(out)
    }

    pub fn snapshot(&mut self) -> Result<SnapshotOut, ApiError> {
        let status = self.status()?;
        if !status.paired {
            return Err(not_paired());
        }
        Ok(SnapshotOut {
            status,
            tasks: self.tasks()?,
            items: self.items()?,
            members: self.members()?,
        })
    }

    // -- finding things by id or name --------------------------------------

    fn find_task(&self, v: &View, r: &str) -> Result<Task, ApiError> {
        let r = r.trim();
        let tasks: Vec<Task> = self.store().list()?;
        let shown: Vec<Task> = tasks.into_iter().filter(|t| Self::shown(v, t)).collect();
        if let Some(t) = shown.iter().find(|t| t.id.to_string() == r) {
            return Ok(t.clone());
        }
        let by_name: Vec<&Task> = shown
            .iter()
            .filter(|t| t.title.eq_ignore_ascii_case(r))
            .collect();
        // Several with one name (two smoke alarms): prefer switched-on ones.
        let active: Vec<&&Task> = by_name.iter().filter(|t| t.active).collect();
        match (by_name.len(), active.len()) {
            (1, _) => Ok(by_name[0].clone()),
            (_, 1) => Ok((*active[0]).clone()),
            (0, _) => Err(not_found(&format!("No task called '{r}'."))),
            (n, _) => Err(ApiError::invalid(format!(
                "{n} tasks are called '{r}'. Use the task's id instead."
            ))),
        }
    }

    fn find_item(&self, v: &View, r: &str) -> Result<Item, ApiError> {
        let r = r.trim();
        let shown: Vec<&Item> = v
            .items
            .values()
            .filter(|i| i.sensitivity == Sensitivity::Normal)
            .collect();
        if let Some(i) = shown.iter().find(|i| i.id.to_string() == r) {
            return Ok((*i).clone());
        }
        let by_name: Vec<&&Item> = shown
            .iter()
            .filter(|i| i.name.eq_ignore_ascii_case(r))
            .collect();
        match by_name.len() {
            1 => Ok((*by_name[0]).clone()),
            0 => Err(not_found(&format!("No thing called '{r}'."))),
            n => Err(ApiError::invalid(format!(
                "{n} things are called '{r}'. Use the thing's id instead."
            ))),
        }
    }

    fn find_member(&self, r: &str) -> Result<Member, ApiError> {
        let r = r.trim();
        let all: Vec<Member> = self.store().list()?;
        if let Some(m) = all.iter().find(|m| m.id.to_string() == r) {
            return Ok(m.clone());
        }
        let by_name: Vec<&Member> = all
            .iter()
            .filter(|m| m.name.eq_ignore_ascii_case(r))
            .collect();
        match by_name.len() {
            1 => Ok(by_name[0].clone()),
            0 => Err(not_found(&format!(
                "No one in the household is called '{r}'."
            ))),
            n => Err(ApiError::invalid(format!(
                "{n} people are called '{r}'. Use their id instead."
            ))),
        }
    }

    // -- changes ----------------------------------------------------------

    /// Mark a task done. `member` is who did it (default: nobody named,
    /// shown as "Home Assistant").
    pub fn complete(&mut self, a: CompleteIn) -> Result<TaskOut, ApiError> {
        self.need_household()?;
        let v = self.view()?;
        let t = self.find_task(&v, &a.task)?;
        let done_by = match a.member.as_deref().filter(|m| !m.trim().is_empty()) {
            Some(m) => Some(self.find_member(m)?.id),
            None => None,
        };
        let at = match a.at {
            Some(at) if at > self.store().now() => {
                return Err(ApiError::invalid("That time is in the future."));
            }
            other => other,
        };
        self.api
            .store_mut()
            .complete_task_for(t.id, at, None, done_by)?;
        let t: Task = self.store().fetch(&t.id.to_string())?;
        self.task_out(&v, &t)
    }

    /// Snooze (reschedule) to a date: `until`, or `days` from today
    /// (default 1).
    pub fn snooze(&mut self, a: SnoozeIn) -> Result<TaskOut, ApiError> {
        self.need_household()?;
        let v = self.view()?;
        let t = self.find_task(&v, &a.task)?;
        let until = match (a.until, a.days) {
            (Some(d), _) => d,
            (None, d) => v
                .today
                .checked_add(Span::new().days(i64::from(d.unwrap_or(1).clamp(1, 366))))
                .map_err(|e| ApiError::invalid(e.to_string()))?,
        };
        if until <= v.today {
            return Err(ApiError::invalid("Snooze to a day after today."));
        }
        if days_between(v.today, until) > 366 {
            return Err(ApiError::invalid("Snooze for at most a year."));
        }
        self.api.call(
            "snooze",
            json!({"id": t.id.to_string(), "until": until.to_string()}),
        )?;
        self.task_out(&v, &t)
    }

    /// A meter reading (runtime hours, loads). With `delta`, the value is
    /// added to the last reading. A value lower than the last one is taken
    /// as a counter that started again from zero (D-121).
    pub fn reading(&mut self, a: ReadingIn) -> Result<ReadingOut, ApiError> {
        self.need_household()?;
        if !a.value.is_finite() || a.value < 0.0 {
            return Err(ApiError::invalid("A reading is a number, zero or more."));
        }
        let v = self.view()?;
        let item = self.find_item(&v, &a.item)?;
        let meters: Vec<Meter> = self.store().list_by("item", &item.id.to_string())?;
        let key = a.meter.trim();
        let meter = meters
            .iter()
            .find(|m| m.key == key || m.name.eq_ignore_ascii_case(key))
            .cloned()
            .ok_or_else(|| {
                let keys: Vec<&str> = meters.iter().map(|m| m.key.as_str()).collect();
                not_found(&if keys.is_empty() {
                    format!("{} has no meters.", item.name)
                } else {
                    format!(
                        "{} has no meter '{key}'. Its meters: {}.",
                        item.name,
                        keys.join(", ")
                    )
                })
            })?;
        let now = self.store().now();
        let last = self
            .store()
            .readings(meter.id)?
            .into_iter()
            .max_by_key(|r| r.at);
        let value = match (&last, a.delta) {
            (Some(l), true) => l.value + a.value,
            (None, true) => a.value,
            (Some(l), false) if a.value < l.value => l.value + a.value,
            _ => a.value,
        };
        let skip = |reason: &str| ReadingOut {
            stored: false,
            reason: Some(reason.into()),
            meter: meter.key.clone(),
            value: last.as_ref().map(|l| l.value).unwrap_or(value),
            tasks: vec![],
        };
        if a.throttle
            && let Some(l) = &last
        {
            if l.value == value {
                return Ok(skip("unchanged"));
            }
            let age = now.as_millisecond() - l.at.as_millisecond();
            if age >= 0 && (age as u128) < THROTTLE.as_millis() {
                return Ok(skip("too soon"));
            }
        }
        self.api.store_mut().record_reading(meter.id, value, None)?;
        let tasks = self.item_tasks(
            &v,
            item.id,
            |s| matches!(s, Schedule::Usage { meter: m, .. } if *m == meter.key),
        )?;
        Ok(ReadingOut {
            stored: true,
            reason: None,
            meter: meter.key,
            value,
            tasks,
        })
    }

    fn item_tasks(
        &self,
        v: &View,
        item: ItemId,
        f: impl Fn(&Schedule) -> bool,
    ) -> Result<Vec<TaskOut>, ApiError> {
        let mut out = Vec::new();
        for t in self.store().list_by::<Task>("item", &item.to_string())? {
            if t.active && f(&t.schedule) {
                out.push(self.task_out(v, &t)?);
            }
        }
        Ok(out)
    }

    /// Something happened to an item ("leak", "battery_low"). Tasks on it
    /// triggered by that event become due; one that was only suggested is
    /// switched on (D-119).
    pub fn event(&mut self, a: EventIn) -> Result<EventOut, ApiError> {
        self.need_household()?;
        let event = a.event.trim().to_lowercase().replace([' ', '-'], "_");
        if !valid_event_name(&event) {
            return Err(ApiError::invalid(
                "An event name is lowercase letters, digits and underscores, like leak or battery_low.",
            ));
        }
        let v = self.view()?;
        let item = self.find_item(&v, &a.item)?;
        let meters: Vec<Meter> = self.store().list_by("item", &item.id.to_string())?;
        let meter = match meters.into_iter().find(|m| m.key == event) {
            Some(m) => m,
            None => {
                let m = Meter {
                    id: MeterId::new(),
                    item: Some(item.id),
                    key: event.clone(),
                    name: format!("{} alerts", sentence(&event.replace('_', " "))),
                    unit: "alerts".into(),
                };
                self.api.store_mut().put(&m)?;
                m
            }
        };
        let now = self.store().now();
        let last = self
            .store()
            .readings(meter.id)?
            .into_iter()
            .max_by_key(|r| r.at);
        let recent = last.as_ref().is_some_and(|l| {
            let age = now.as_millisecond() - l.at.as_millisecond();
            age >= 0 && (age as u128) < EVENT_DEDUPE.as_millis()
        });
        let mut switched_on = Vec::new();
        if !recent {
            let n = last.map(|l| l.value).unwrap_or(0.0) + 1.0;
            self.api.store_mut().record_reading(meter.id, n, None)?;
        }
        let mine: Vec<Task> = self
            .store()
            .list_by::<Task>("item", &item.id.to_string())?
            .into_iter()
            .filter(|t| matches!(&t.schedule, Schedule::Triggered { event: e } if *e == event))
            .collect();
        if !mine.iter().any(|t| t.active) {
            for mut t in mine.into_iter().filter(|t| !t.paused && !t.not_applicable) {
                t.active = true;
                self.api.store_mut().put(&t)?;
                switched_on.push(t.id.to_string());
            }
        }
        let tasks = self.item_tasks(
            &v,
            item.id,
            |s| matches!(s, Schedule::Triggered { event: e } if *e == event),
        )?;
        let message = match (recent, tasks.is_empty()) {
            (true, _) => "Already reported in the last few minutes.".to_string(),
            (false, true) => format!(
                "Recorded. No task on {} is set to follow a {} alert.",
                item.name,
                event.replace('_', " ")
            ),
            (false, false) => "Recorded.".to_string(),
        };
        Ok(EventOut {
            recorded: !recent,
            event,
            message,
            switched_on,
            tasks,
        })
    }

    // -- devices from Home Assistant ---------------------------------------

    fn template_names(&mut self) -> Result<Vec<(String, String)>, ApiError> {
        let t = self.api.call("templates", Value::Null)?;
        Ok(t.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| {
                        Some((
                            t["id"].as_str()?.to_string(),
                            t["name"].as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn existing(&self, v: &View, d: &DeviceIn) -> Option<String> {
        v.items
            .values()
            .find(|i| {
                i.name.eq_ignore_ascii_case(d.name.trim())
                    && i.make.as_deref().unwrap_or("") == d.manufacturer.as_deref().unwrap_or("")
                    && i.model.as_deref().unwrap_or("") == d.model.as_deref().unwrap_or("")
            })
            .map(|i| i.id.to_string())
    }

    /// Which kind of thing each device looks like, and whether it is
    /// already recorded.
    pub fn match_devices(&mut self, devices: Vec<DeviceIn>) -> Result<Vec<DeviceMatch>, ApiError> {
        self.need_household()?;
        let names = self.template_names()?;
        let v = self.view()?;
        Ok(devices
            .into_iter()
            .map(|d| {
                let template = matching::match_template(&d, &names);
                DeviceMatch {
                    template_name: template
                        .as_ref()
                        .and_then(|t| names.iter().find(|(id, _)| id == t))
                        .map(|(_, n)| n.clone()),
                    existing_item: self.existing(&v, &d),
                    template,
                    id: d.id,
                    name: d.name,
                }
            })
            .collect())
    }

    /// Add devices as things, each with its kind's tasks as suggestions
    /// (switched off; D-120).
    pub fn import_devices(&mut self, devices: Vec<DeviceIn>) -> Result<ImportOut, ApiError> {
        self.need_household()?;
        if devices.len() > 200 {
            return Err(ApiError::invalid("Import at most 200 devices at a time."));
        }
        let names = self.template_names()?;
        let mut out = ImportOut::default();
        for d in devices {
            let v = self.view()?;
            if d.name.trim().is_empty() {
                out.skipped.push(Skipped {
                    id: d.id,
                    reason: "no name".into(),
                });
                continue;
            }
            if self.existing(&v, &d).is_some() {
                out.skipped.push(Skipped {
                    id: d.id,
                    reason: "already recorded".into(),
                });
                continue;
            }
            let template = d
                .template
                .clone()
                .filter(|t| names.iter().any(|(id, _)| id == t))
                .or_else(|| matching::match_template(&d, &names));
            let Some(template) = template else {
                out.skipped.push(Skipped {
                    id: d.id,
                    reason: "no matching kind of thing".into(),
                });
                continue;
            };
            let added = self.api.call(
                "add_from_template",
                json!({
                    "template": template,
                    "name": d.name.trim(),
                    "make": d.manufacturer,
                    "model": d.model,
                    "new_room": d.area,
                    "values": {},
                }),
            )?;
            out.added.push(Imported {
                id: d.id,
                item_id: added["item_id"].as_str().unwrap_or_default().to_string(),
                name: d.name.trim().to_string(),
                template,
                suggested_tasks: added["tasks"].as_array().map_or(0, |a| a.len()),
            });
        }
        Ok(out)
    }

    // -- pairing ----------------------------------------------------------

    /// Start joining with the code the app shows.
    pub fn pair_start(&mut self, a: PairIn) -> Result<Value, ApiError> {
        let name = a
            .device_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(DEFAULT_DEVICE_NAME)
            .to_string();
        let s = self.api.call(
            "sync_join",
            json!({"server": a.server.unwrap_or_default(), "code": a.code, "device_name": name}),
        )?;
        let role = self.api.sync_client().and_then(|c| c.join_offer_role());
        if matches!(role, Some(DeviceRole::Owner) | Some(DeviceRole::Recovery)) {
            let _ = self
                .api
                .call("sync_join_confirm", json!({"matches": false}));
            return Err(ApiError::invalid(
                "That code adds another device of the household's owner, which would give Home Assistant an owner's powers. In the app, choose Add a device or person, then Home Assistant, and use that code.",
            ));
        }
        Ok(s)
    }

    pub fn pair_poll(&mut self) -> Result<Value, ApiError> {
        self.api.call("sync_join_poll", Value::Null)
    }

    pub fn pair_confirm(&mut self, matches: bool) -> Result<Value, ApiError> {
        self.api
            .call("sync_join_confirm", json!({"matches": matches}))
    }

    // -- a demo household --------------------------------------------------

    /// A sample household with no sync, for trying the integration and for
    /// tests (`housewright-bridge demo`). Includes sensitive things the API
    /// must never show.
    pub fn seed_demo(&mut self) -> Result<(), ApiError> {
        if self.is_paired() {
            return Err(ApiError::invalid("This bridge already has a household."));
        }
        let api = &mut self.api;
        api.call(
            "init",
            json!({"property_name": "Demo house", "your_name": "Ana", "household_name": "Demo household", "time_zone": "America/New_York"}),
        )?;
        for (t, room) in [
            ("washing-machine", "Laundry"),
            ("smoke-alarm", "Hallway"),
            ("water-heater", "Basement"),
        ] {
            api.call(
                "add_from_template",
                json!({"template": t, "new_room": room, "values": if t == "smoke-alarm" { json!({"power": "battery"}) } else { json!({}) }}),
            )?;
        }
        let ana = api.store().author()?;
        let store = api.store_mut();
        let property = store.list::<Property>()?.remove(0);
        let ben = Member {
            id: MemberId::new(),
            name: "Ben".into(),
            role: Role::Member,
            kind: MemberKind::Person,
            contact: Some(Flagged::sensitive("ben-SECRET-CONTACT@example.com".into())),
            reminders: None,
        };
        store.put(&ben)?;
        // Things the API must never show (D-41).
        let safe = Item {
            id: ItemId::new(),
            property: property.id,
            room: None,
            parent: None,
            name: "SECRET-SAFE-NAME".into(),
            category: None,
            make: None,
            model: None,
            serial: Some(Flagged::sensitive("SECRET-SERIAL-123".into())),
            purchased: None,
            installed: None,
            warranty: None,
            location_note: Some(Flagged::sensitive("SECRET-LOCATION-behind-mirror".into())),
            template: None,
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Sensitivity::Sensitive,
            price: Default::default(),
            value: Default::default(),
            family: None,
        };
        store.put(&safe)?;
        store.put(&Task {
            id: TaskId::new(),
            property: property.id,
            item: Some(safe.id),
            room: None,
            title: "SECRET-SAFE-TASK".into(),
            owner: ana,
            priority: housewright_core::priority::PriorityClass::Cosmetic,
            schedule: Schedule::Floating {
                every: "1 month".parse().map_err(ApiError::invalid)?,
            },
            window: None,
            start: civil_date(store.now(), &TimeZone::get(&property.time_zone)?),
            steps: vec![],
            why: None,
            minutes: None,
            completion_fields: vec![],
            template: None,
            active: true,
            paused: false,
            not_applicable: false,
            guidance: None,
        })?;
        store.put(&Fact {
            pin: None,
            id: FactId::new(),
            property: property.id,
            category: "codes".into(),
            label: "Alarm code".into(),
            value: "SECRET-CODE-4321".into(),
            sensitivity: Sensitivity::Sensitive,
            key: None,
            room: None,
        })?;
        let mut items: Vec<Item> = store.list()?;
        for i in items
            .iter_mut()
            .filter(|i| i.template.as_deref() == Some("washing-machine"))
        {
            i.serial = Some(Flagged::sensitive("SECRET-WASHER-SERIAL".into()));
            store.put(i)?;
        }
        // Everything switched on except the triggered tasks, which stay
        // suggestions until their event arrives (D-119).
        for mut t in store.list::<Task>()? {
            if !matches!(t.schedule, Schedule::Triggered { .. }) {
                t.active = true;
                store.put(&t)?;
            }
        }
        // From here on, the bridge acts as Home Assistant.
        let ha = Member {
            id: MemberId::new(),
            name: "Home Assistant".into(),
            role: Role::Member,
            kind: MemberKind::HomeAssistant,
            contact: None,
            reminders: None,
        };
        store.put(&ha)?;
        store.set_author(ha.id)?;
        Ok(())
    }
}

struct View {
    items: BTreeMap<ItemId, Item>,
    rooms: BTreeMap<RoomId, String>,
    members: BTreeMap<MemberId, String>,
    today: Date,
}

/// The states Home Assistant sees.
pub fn state_key(s: TaskState) -> &'static str {
    match s {
        TaskState::Scheduled | TaskState::Completed => "ok",
        TaskState::DueSoon => "due_soon",
        TaskState::Due => "due",
        TaskState::Overdue => "overdue",
        TaskState::Snoozed => "snoozed",
        TaskState::Paused | TaskState::NotApplicable => "paused",
    }
}

/// All the states, for Home Assistant's enum sensor.
pub const STATES: &[&str] = &["ok", "due_soon", "due", "overdue", "snoozed", "paused"];

fn days_between(a: Date, b: Date) -> i64 {
    a.until(b).map(|s| i64::from(s.get_days())).unwrap_or(0)
}

fn sentence(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Make sure `at` (when given) is not used to rewrite history far back.
pub fn parse_at(s: Option<&str>) -> Result<Option<Timestamp>, ApiError> {
    match s.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => s
            .parse::<Timestamp>()
            .map(Some)
            .map_err(|e| ApiError::invalid(format!("'{s}' is not a time: {e}"))),
    }
}
