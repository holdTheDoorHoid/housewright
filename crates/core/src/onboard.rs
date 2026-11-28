//! Household-level onboarding and growth, built on [`Store`]: save the home
//! profile, turn it into things and suggested tasks, switch on a small
//! starting plan, and answer the weekly suggestion (D-65 to D-67).

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::climate::{ClimateLookup, lookup_us_zip};
use crate::error::{Error, Result};
use crate::fields::{CustomValue, FieldType, FieldValue};
use crate::ids::*;
use crate::model::*;
use crate::priority::{self, StartCandidate};
use crate::profile::{PlannedThing, Profile, plan_home};
use crate::schedule::civil_date;
use crate::store::Store;
use crate::suggest::{
    self, Answer, Dormant, SuggestInput, Suggestion, SuggestionAction, SuggestionAnswer,
};
use crate::template::{Library, Template};

/// Templates whose tests go ahead of the rest of life safety.
pub const ALARM_TEMPLATES: [&str; 2] = ["smoke-alarm", "co-alarm"];

/// Order within a priority class for the starting plan
/// ([`StartCandidate::hint`]).
pub fn start_hint(template: Option<&str>, category: Option<&str>) -> u8 {
    if template.is_some_and(|t| ALARM_TEMPLATES.contains(&t)) {
        0
    } else if matches!(category, Some("exterior" | "structure" | "yard")) {
        2
    } else {
        1
    }
}

/// A floating task that has never been done counts as due the day it is
/// added, only because nobody knows when it was last done (D-54). For
/// choosing the first tasks that is not real urgency, so rank it as
/// scheduled.
fn ranking_status(t: &Task, mut s: crate::priority::TaskStatus) -> crate::priority::TaskStatus {
    use crate::schedule::{Schedule, TaskState};
    if matches!(
        t.schedule,
        Schedule::Floating { .. } | Schedule::Usage { .. }
    ) && s.eval.last_done.is_none()
        && matches!(
            s.eval.state,
            TaskState::Due | TaskState::Overdue | TaskState::DueSoon
        )
    {
        s.eval.state = TaskState::Scheduled;
    }
    s
}

/// What applying a profile did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyOutcome {
    /// Things created this time (things already there are left alone).
    pub new_items: Vec<ItemId>,
    pub new_tasks: Vec<TaskId>,
    /// The climate looked up from the ZIP code, if any.
    pub climate: Option<ClimateLookup>,
}

impl Store {
    /// The profile of a property, if the questions have been answered.
    pub fn profile(&self, property: PropertyId) -> Result<Option<Profile>> {
        self.get(&property.to_string())
    }

    /// Save the answers. A ZIP code that is in the bundled table sets the
    /// property's climate (frost dates, hard freeze, hemisphere).
    pub fn save_profile(&mut self, profile: &Profile) -> Result<Option<ClimateLookup>> {
        let mut property: Property = self.fetch(&profile.id.to_string())?;
        self.put(profile)?;
        let lookup = profile.zip.as_ref().and_then(|z| lookup_us_zip(&z.value));
        if let Some(l) = &lookup
            && property.climate != l.climate
        {
            property.climate = l.climate.clone();
            self.put(&property)?;
        }
        Ok(lookup)
    }

    /// Save the profile and add every thing it implies that is not recorded
    /// yet, each with its suggested tasks switched off. Never deletes or
    /// changes existing things or tasks, so answering again only adds.
    pub fn apply_profile(
        &mut self,
        profile: &Profile,
        library: &Library,
        owner: MemberId,
        start: Date,
    ) -> Result<ApplyOutcome> {
        let climate = self.save_profile(profile)?;
        let existing: Vec<Item> = self.list()?;
        let mut rooms: Vec<Room> = self.list()?;
        let mut out = ApplyOutcome {
            climate,
            ..ApplyOutcome::default()
        };
        for planned in plan_home(profile) {
            if existing.iter().any(|i| {
                i.property == profile.id && i.template.as_deref() == Some(&planned.template)
            }) {
                continue;
            }
            let template = library.resolve(&planned.template)?;
            let room = match &planned.room {
                Some(name) => Some(self.room_named(profile.id, name, &mut rooms)?),
                None => None,
            };
            let item = item_from_plan(profile.id, &planned, &template, room)?;
            let tasks = self.add_item_from_template(&item, &template, owner, start)?;
            out.new_items.push(item.id);
            out.new_tasks.extend(tasks.iter().map(|t| t.id));
        }
        Ok(out)
    }

    /// Find a room by name (any case) or create it.
    fn room_named(
        &mut self,
        property: PropertyId,
        name: &str,
        rooms: &mut Vec<Room>,
    ) -> Result<RoomId> {
        if let Some(r) = rooms
            .iter()
            .find(|r| r.property == property && r.name.eq_ignore_ascii_case(name))
        {
            return Ok(r.id);
        }
        let r = Room {
            id: RoomId::new(),
            property,
            floor: None,
            name: name.to_string(),
        };
        self.put(&r)?;
        let id = r.id;
        rooms.push(r);
        Ok(id)
    }

    /// P3 after onboarding: switch on the `n` best suggested tasks of a
    /// property ([`priority::starting_plan`]). Returns them in plan order.
    pub fn switch_on_starting_plan(
        &mut self,
        property: PropertyId,
        n: usize,
        now: Timestamp,
    ) -> Result<Vec<TaskId>> {
        let items: BTreeMap<ItemId, Item> = self
            .list::<Item>()?
            .into_iter()
            .map(|i| (i.id, i))
            .collect();
        let mut cands = Vec::new();
        for t in self.list::<Task>()? {
            if t.active || t.property != property {
                continue;
            }
            let template = t
                .template
                .as_ref()
                .map(|r| r.template.as_str())
                .or_else(|| t.item.and_then(|i| items.get(&i)?.template.as_deref()));
            cands.push(StartCandidate {
                status: ranking_status(&t, self.task_status(&t, now)?),
                item: t.item,
                period_days: t.schedule.period_days(),
                hint: start_hint(
                    template,
                    t.item.and_then(|i| items.get(&i)?.category.as_deref()),
                ),
            });
        }
        let picked = priority::starting_plan(&cands, n);
        for id in &picked {
            let mut t: Task = self.fetch(&id.to_string())?;
            t.active = true;
            self.put(&t)?;
        }
        Ok(picked)
    }

    pub fn suggestion_answers(&self, property: PropertyId) -> Result<Vec<SuggestionAnswer>> {
        self.list_by("property", &property.to_string())
    }

    /// Every eligible suggestion for a property, best first (ignores the
    /// once-a-week limit), and whether one was answered this week.
    pub fn suggestions(
        &self,
        property: PropertyId,
        now: Timestamp,
    ) -> Result<(Vec<Suggestion>, bool)> {
        let p: Property = self.fetch(&property.to_string())?;
        let tz = TimeZone::get(&p.time_zone)?;
        let profile = self.profile(property)?;
        let items: Vec<Item> = self
            .list::<Item>()?
            .into_iter()
            .filter(|i| i.property == property)
            .collect();
        let present: Vec<String> = items.iter().filter_map(|i| i.template.clone()).collect();
        let tasks: Vec<Task> = self
            .list::<Task>()?
            .into_iter()
            .filter(|t| t.property == property && !t.active && !t.paused && !t.not_applicable)
            .collect();
        let mut statuses = Vec::new();
        for t in &tasks {
            statuses.push(self.task_status(t, now)?);
        }
        let dormant: Vec<Dormant<'_>> = tasks
            .iter()
            .zip(statuses.iter())
            .map(|(t, s)| Dormant {
                task: t,
                status: s,
                item_name: t
                    .item
                    .and_then(|i| items.iter().find(|x| x.id == i))
                    .map(|i| i.name.as_str()),
            })
            .collect();
        let answers = self.suggestion_answers(property)?;
        let input = SuggestInput {
            profile: profile.as_ref(),
            climate: &p.climate,
            templates_present: &present,
            dormant: &dormant,
            answers: &answers,
            today: civil_date(now, &tz),
            now,
        };
        Ok((
            suggest::candidates(&input),
            suggest::answered_this_week(&answers, now),
        ))
    }

    /// This week's single suggestion, if any.
    pub fn weekly_suggestion(
        &self,
        property: PropertyId,
        now: Timestamp,
    ) -> Result<Option<Suggestion>> {
        let (all, answered) = self.suggestions(property, now)?;
        Ok(if answered {
            None
        } else {
            all.into_iter().next()
        })
    }

    /// Record an answer. "Added" also does it: adds the thing and switches
    /// on its most relevant task, or switches the suggested task on. Returns
    /// the tasks switched on.
    pub fn answer_suggestion(
        &mut self,
        property: PropertyId,
        key: &str,
        answer: Answer,
        library: &Library,
        owner: MemberId,
        now: Timestamp,
    ) -> Result<Vec<TaskId>> {
        let (all, _) = self.suggestions(property, now)?;
        let s = all
            .into_iter()
            .find(|s| s.key == key)
            .ok_or_else(|| Error::NotFound(format!("suggestion '{key}'")))?;
        let mut switched = Vec::new();
        if answer == Answer::Added {
            match &s.action {
                SuggestionAction::SwitchOn { task } => {
                    let mut t: Task = self.fetch(&task.to_string())?;
                    t.active = true;
                    self.put(&t)?;
                    switched.push(t.id);
                }
                SuggestionAction::AddThing {
                    template,
                    name,
                    season,
                } => {
                    let p: Property = self.fetch(&property.to_string())?;
                    let tz = TimeZone::get(&p.time_zone)?;
                    let start = civil_date(now, &tz);
                    let resolved = library.resolve(template)?;
                    let planned = PlannedThing {
                        template: template.clone(),
                        name: name.clone(),
                        room: None,
                        values: BTreeMap::new(),
                    };
                    let item = item_from_plan(property, &planned, &resolved, None)?;
                    let tasks = self.add_item_from_template(&item, &resolved, owner, start)?;
                    // The task the question was about (the seasonal one),
                    // otherwise the best one for this thing.
                    let chosen = tasks
                        .iter()
                        .find(|t| season.is_some() && t.schedule.season() == *season)
                        .map(|t| t.id);
                    let chosen = match chosen {
                        Some(id) => Some(id),
                        None => {
                            let mut cands = Vec::new();
                            for t in &tasks {
                                cands.push(StartCandidate {
                                    status: self.task_status(t, now)?,
                                    item: t.item,
                                    period_days: t.schedule.period_days(),
                                    hint: 1,
                                });
                            }
                            priority::starting_plan(&cands, 1).into_iter().next()
                        }
                    };
                    if let Some(id) = chosen {
                        let mut t: Task = self.fetch(&id.to_string())?;
                        t.active = true;
                        self.put(&t)?;
                        switched.push(id);
                    }
                }
            }
        }
        self.append(&SuggestionAnswer {
            id: SuggestionAnswerId::new(),
            property,
            key: key.to_string(),
            answer,
            by: self.author()?,
            at: now,
        })?;
        Ok(switched)
    }
}

/// Build the item for a planned thing, with its field values typed by the
/// template's field definitions.
fn item_from_plan(
    property: PropertyId,
    planned: &PlannedThing,
    template: &Template,
    room: Option<RoomId>,
) -> Result<Item> {
    let mut custom = BTreeMap::new();
    for (k, raw) in &planned.values {
        let def = template
            .fields
            .iter()
            .find(|f| &f.key == k)
            .ok_or_else(|| {
                Error::Invalid(format!("template {} has no field '{k}'", template.id))
            })?;
        let value = match &def.field_type {
            FieldType::Choice { .. } => FieldValue::Choice(raw.clone()),
            FieldType::YesNo => FieldValue::YesNo(raw == "yes"),
            FieldType::Text => FieldValue::Text(raw.clone()),
            other => {
                return Err(Error::Invalid(format!(
                    "field '{k}' of type {other:?} cannot be set from the questions"
                )));
            }
        };
        def.check(&value).map_err(Error::Invalid)?;
        custom.insert(
            k.clone(),
            CustomValue {
                value,
                sensitivity: def.sensitivity,
            },
        );
    }
    Ok(Item {
        id: ItemId::new(),
        property,
        room,
        parent: None,
        name: planned.name.clone(),
        category: template.category.clone(),
        make: None,
        model: None,
        serial: None,
        purchased: None,
        installed: None,
        warranty: None,
        location_note: None,
        template: Some(template.id.clone()),
        extra_fields: vec![],
        custom,
        sensitivity: Sensitivity::Normal,
    })
}
