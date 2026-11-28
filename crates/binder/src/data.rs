//! Everything the binder prints about one property, read from the store
//! once. Builders work on this snapshot, never on the store, so they are
//! pure and easy to test.
//!
//! The snapshot keeps every sensitivity flag. Two things are dropped here,
//! before any builder can see them, because neither volume ever prints
//! them: a contractor's private rating (D-28) and members' contact details.

use std::collections::BTreeMap;

use housewright_core::Result;
use housewright_core::costs::effective_costs;
use housewright_core::fields::FieldDef;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::profile::Profile;
use housewright_core::schedule::{Evaluation, TaskState, civil_date};
use housewright_core::store::Store;
use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

/// The fact key that says where the sealed volume is kept (D-41).
pub const SEALED_LOCATION_KEY: &str = "sealed_volume_location";

#[derive(Clone, Debug)]
pub struct TaskData {
    pub task: Task,
    pub eval: Evaluation,
    /// Due dates in the next twelve months (the first may be in the past
    /// when the task is overdue).
    pub upcoming: Vec<Date>,
}

#[derive(Clone, Debug)]
pub struct ItemData {
    pub item: Item,
    pub fields: Vec<FieldDef>,
    /// Switched-on tasks only: the plan, not the suggestions.
    pub tasks: Vec<TaskData>,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug)]
pub struct FactData {
    pub fact: Fact,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug)]
pub struct CompletionData {
    pub at: Date,
    pub by: MemberId,
    pub task_title: String,
    pub item: Option<ItemId>,
    pub contractor: Option<ContractorId>,
    pub notes: Option<Flagged<String>>,
    pub costs: Vec<CostLine>,
}

#[derive(Clone, Debug)]
pub struct NoteData {
    pub id: NoteId,
    pub about: EntityRef,
    pub at: Timestamp,
    pub text: Flagged<String>,
}

/// What a redaction took out of the main volume, as counts only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Withheld {
    /// Facts per group key.
    pub facts: BTreeMap<String, usize>,
    pub items: usize,
    pub item_details: usize,
    pub notes: usize,
    pub contractor_details: usize,
}

impl Withheld {
    pub fn any(&self) -> bool {
        self.facts.values().sum::<usize>()
            + self.items
            + self.item_details
            + self.notes
            + self.contractor_details
            > 0
    }
}

#[derive(Clone, Debug)]
pub struct HouseData {
    pub today: Date,
    pub household_name: String,
    pub currency: String,
    pub property: Property,
    pub profile: Option<Profile>,
    pub rooms: BTreeMap<RoomId, String>,
    pub members: BTreeMap<MemberId, String>,
    pub items: Vec<ItemData>,
    /// Switched-on tasks that are not about one thing.
    pub house_tasks: Vec<TaskData>,
    pub facts: Vec<FactData>,
    pub contractors: Vec<Contractor>,
    /// Newest first.
    pub completions: Vec<CompletionData>,
    /// Notes about the house, a room or a thing: "quirks only you know".
    pub notes: Vec<NoteData>,
    /// Set by [`crate::redact`] on the public copy.
    pub withheld: Withheld,
}

impl HouseData {
    pub fn room_name(&self, r: Option<RoomId>) -> Option<&str> {
        r.and_then(|r| self.rooms.get(&r)).map(|s| s.as_str())
    }

    pub fn member_name(&self, m: MemberId) -> &str {
        self.members
            .get(&m)
            .map(|s| s.as_str())
            .unwrap_or("someone")
    }

    pub fn sealed_location(&self) -> Option<&str> {
        self.facts
            .iter()
            .find(|f| f.fact.key.as_deref() == Some(SEALED_LOCATION_KEY))
            .map(|f| f.fact.value.as_str())
            .filter(|v| !v.trim().is_empty())
    }

    pub fn notes_about(&self, kind: EntityKind, id: &str) -> Vec<&NoteData> {
        let mut v: Vec<&NoteData> = self
            .notes
            .iter()
            .filter(|n| n.about.kind == kind && n.about.id == id)
            .collect();
        v.sort_by_key(|n| (n.at, n.id));
        v
    }
}

/// Due dates over the next year: evaluate, pretend it was done on the due
/// date, evaluate again. Uses the real schedule engine, so every kind of
/// schedule is projected exactly as the app would remind about it.
fn project(store: &Store, task: &Task, now: Timestamp, tz: &TimeZone) -> Result<Vec<Date>> {
    let today = civil_date(now, tz);
    let horizon = today
        .checked_add(jiff::Span::new().days(365))
        .unwrap_or(today);
    let status = store.task_status(task, now)?;
    let mut out = Vec::new();
    let Some(w) = status.eval.window else {
        return Ok(out);
    };
    if !status.eval.state.is_live() {
        return Ok(out);
    }
    let mut due = w.due;
    // Later occurrences: add pretend completions on each due date.
    let mut pretend: Vec<Timestamp> = store
        .effective_completions(task.id)?
        .iter()
        .map(|c| c.at)
        .collect();
    for _ in 0..60 {
        if due >= horizon {
            break;
        }
        out.push(due);
        if matches!(
            task.schedule,
            housewright_core::schedule::Schedule::OneOff { .. }
        ) {
            break;
        }
        // Done at noon on the due date (or today, if it is overdue).
        let done_on = due.max(today);
        let Ok(at) = tz.to_timestamp(done_on.at(12, 0, 0, 0)) else {
            break;
        };
        pretend.push(at);
        let input = housewright_core::schedule::ScheduleInput {
            schedule: &task.schedule,
            window: task.window,
            start: task.start,
            assumed_last_done: None,
            completions: &pretend,
            snoozes: &[],
            readings: &[],
            climate: &store.fetch::<Property>(&task.property.to_string())?.climate,
            paused: false,
            not_applicable: false,
        };
        let e = housewright_core::schedule::evaluate(&input, at, tz);
        match e.window {
            Some(w) if w.due > due => due = w.due,
            _ => break,
        }
    }
    Ok(out)
}

fn task_data(store: &Store, t: Task, now: Timestamp, tz: &TimeZone) -> Result<TaskData> {
    let eval = store.task_status(&t, now)?.eval;
    let upcoming = project(store, &t, now, tz)?;
    Ok(TaskData {
        task: t,
        eval,
        upcoming,
    })
}

/// Read one property's binder data. `property` defaults to the first.
pub fn gather(store: &Store, property: Option<PropertyId>, now: Timestamp) -> Result<HouseData> {
    let household = store.household()?;
    let props: Vec<Property> = store.list()?;
    let property = match property {
        Some(p) => store.fetch::<Property>(&p.to_string())?,
        None => props
            .into_iter()
            .next()
            .ok_or_else(|| housewright_core::Error::NotFound("property".into()))?,
    };
    let pid = property.id.to_string();
    let tz = TimeZone::get(&property.time_zone).unwrap_or(TimeZone::UTC);
    let today = civil_date(now, &tz);

    let rooms: BTreeMap<RoomId, String> = store
        .list_by::<Room>("property", &pid)?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();
    let members: BTreeMap<MemberId, String> = store
        .list::<Member>()?
        .into_iter()
        .map(|m| (m.id, m.name))
        .collect();
    let attachments: Vec<Attachment> = store.list()?;
    let about = |kind: EntityKind, id: &str| -> Vec<Attachment> {
        attachments
            .iter()
            .filter(|a| a.about.kind == kind && a.about.id == id)
            .cloned()
            .collect()
    };

    let tasks: Vec<Task> = store.list_by("property", &pid)?;
    let live = |t: &Task| t.active && !t.paused && !t.not_applicable;
    let mut items = Vec::new();
    for it in store.list_by::<Item>("property", &pid)? {
        let mut its: Vec<TaskData> = Vec::new();
        for t in tasks.iter().filter(|t| t.item == Some(it.id) && live(t)) {
            its.push(task_data(store, t.clone(), now, &tz)?);
        }
        its.sort_by(|a, b| a.task.title.cmp(&b.task.title));
        items.push(ItemData {
            fields: store.field_defs(&it)?,
            attachments: about(EntityKind::Item, &it.id.to_string()),
            tasks: its,
            item: it,
        });
    }
    items.sort_by_key(|a| a.item.name.to_lowercase());
    let mut house_tasks = Vec::new();
    for t in tasks.iter().filter(|t| t.item.is_none() && live(t)) {
        house_tasks.push(task_data(store, t.clone(), now, &tz)?);
    }

    let mut facts: Vec<FactData> = store
        .list_by::<Fact>("property", &pid)?
        .into_iter()
        .map(|f| FactData {
            attachments: about(EntityKind::Fact, &f.id.to_string()),
            fact: f,
        })
        .collect();
    facts.sort_by(|a, b| {
        a.fact
            .label
            .to_lowercase()
            .cmp(&b.fact.label.to_lowercase())
    });

    let mut contractors: Vec<Contractor> = store.list()?;
    for c in &mut contractors {
        // Never printed, in either volume (D-28).
        c.private_rating = None;
    }
    contractors.sort_by_key(|c| c.name.to_lowercase());

    let details: Vec<CompletionDetail> = store.list()?;
    let by_task: BTreeMap<TaskId, &Task> = tasks.iter().map(|t| (t.id, t)).collect();
    let mut completions = Vec::new();
    for t in &tasks {
        for c in store.effective_completions(t.id)? {
            completions.push(CompletionData {
                at: civil_date(c.at, &tz),
                by: c.by,
                task_title: by_task[&c.task].title.clone(),
                item: t.item,
                contractor: c.contractor,
                costs: effective_costs(&c, &details),
                notes: c.notes,
            });
        }
    }
    completions.sort_by(|a, b| b.at.cmp(&a.at).then(a.task_title.cmp(&b.task_title)));

    let item_ids: Vec<String> = items.iter().map(|i| i.item.id.to_string()).collect();
    let notes: Vec<NoteData> = store
        .list::<Note>()?
        .into_iter()
        .filter(|n| match n.about.kind {
            EntityKind::Property => n.about.id == pid,
            EntityKind::Room => rooms.keys().any(|r| r.to_string() == n.about.id),
            EntityKind::Item => item_ids.contains(&n.about.id),
            _ => false,
        })
        .map(|n| NoteData {
            id: n.id,
            about: n.about,
            at: n.at,
            text: n.text,
        })
        .collect();

    Ok(HouseData {
        today,
        household_name: household.name.clone(),
        currency: household.currency().to_string(),
        profile: store.get::<Profile>(&pid)?,
        property,
        rooms,
        members,
        items,
        house_tasks,
        facts,
        contractors,
        completions,
        notes,
        withheld: Withheld::default(),
    })
}

/// Whether a task's state means it is waiting to be done now.
pub fn is_due_now(s: TaskState) -> bool {
    matches!(s, TaskState::Due | TaskState::Overdue)
}
