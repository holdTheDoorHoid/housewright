//! The repository layer: household-level operations built on [`Store`]. This
//! is what front ends (the CLI now, the app later) call.

use std::collections::BTreeSet;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::error::{Error, Result};
use crate::ids::*;
use crate::model::*;
use crate::priority::{self, TaskStatus};
use crate::schedule::{self, ReadingInput, ScheduleInput, SnoozeInput};
use crate::store::Store;
use crate::template::Template;

impl Store {
    /// Set up a new household in an empty database, with its first member as
    /// owner. That member becomes this device's author.
    pub fn init_household(
        &mut self,
        name: &str,
        owner_name: &str,
        time_zone: &str,
    ) -> Result<(Household, Member)> {
        if self.meta("household_id")?.is_some() {
            return Err(Error::Invalid(
                "this database already has a household".into(),
            ));
        }
        TimeZone::get(time_zone)
            .map_err(|e| Error::Invalid(format!("time zone '{time_zone}': {e}")))?;
        let owner = Member {
            id: MemberId::new(),
            name: owner_name.to_string(),
            role: Role::Owner,
            contact: None,
            reminders: None,
        };
        let household = Household {
            id: HouseholdId::new(),
            name: name.to_string(),
            time_zone: time_zone.to_string(),
        };
        self.set_author(owner.id)?;
        self.set_meta("household_id", &household.id.to_string())?;
        self.put(&household)?;
        self.put(&owner)?;
        Ok((household, owner))
    }

    pub fn household(&self) -> Result<Household> {
        let id = self
            .meta("household_id")?
            .ok_or_else(|| Error::NotFound("household (run init first)".into()))?;
        self.fetch(&id)
    }

    /// Display name of a member, or a short id if unknown.
    pub fn member_name(&self, id: MemberId) -> String {
        match self.get::<Member>(&id.to_string()) {
            Ok(Some(m)) => m.name,
            _ => format!("member {}", &id.to_string()[..8]),
        }
    }

    /// Completions of a task that still count: voided ones and the voiding
    /// records themselves are left out.
    pub fn effective_completions(&self, task: TaskId) -> Result<Vec<Completion>> {
        let all: Vec<Completion> = self.list_by("task", &task.to_string())?;
        let voided: BTreeSet<CompletionId> = all.iter().filter_map(|c| c.voids).collect();
        let mut out: Vec<Completion> = all
            .into_iter()
            .filter(|c| c.voids.is_none() && !voided.contains(&c.id))
            .collect();
        out.sort_by_key(|c| c.at);
        Ok(out)
    }

    pub fn snoozes(&self, task: TaskId) -> Result<Vec<Snooze>> {
        self.list_by("task", &task.to_string())
    }

    /// The meter a usage task reads: the one with the schedule's key on the
    /// task's item.
    pub fn meter_for(&self, task: &Task) -> Result<Option<Meter>> {
        let schedule::Schedule::Usage { meter, .. } = &task.schedule else {
            return Ok(None);
        };
        let Some(item) = task.item else {
            return Ok(None);
        };
        let meters: Vec<Meter> = self.list_by("item", &item.to_string())?;
        Ok(meters.into_iter().find(|m| &m.key == meter))
    }

    pub fn readings(&self, meter: MeterId) -> Result<Vec<MeterReading>> {
        self.list_by("meter", &meter.to_string())
    }

    /// Run the schedule engine for one task.
    pub fn task_status(&self, task: &Task, now: Timestamp) -> Result<TaskStatus> {
        let property: Property = self.fetch(&task.property.to_string())?;
        let tz = TimeZone::get(&property.time_zone)?;
        let completions: Vec<Timestamp> = self
            .effective_completions(task.id)?
            .iter()
            .map(|c| c.at)
            .collect();
        let snoozes: Vec<SnoozeInput> = self
            .snoozes(task.id)?
            .iter()
            .map(|s| SnoozeInput {
                at: s.at,
                until: s.until,
            })
            .collect();
        let readings: Vec<ReadingInput> = match self.meter_for(task)? {
            Some(m) => self
                .readings(m.id)?
                .iter()
                .map(|r| ReadingInput {
                    at: r.at,
                    value: r.value,
                })
                .collect(),
            None => vec![],
        };
        let assumed_last_done = match task.item {
            Some(i) => self
                .get::<Item>(&i.to_string())?
                .and_then(|it| it.installed.or(it.purchased)),
            None => None,
        };
        let input = ScheduleInput {
            schedule: &task.schedule,
            window: task.window,
            start: task.start,
            assumed_last_done,
            completions: &completions,
            snoozes: &snoozes,
            readings: &readings,
            climate: &property.climate,
            paused: task.paused,
            not_applicable: task.not_applicable,
        };
        Ok(TaskStatus {
            task: task.id,
            title: task.title.clone(),
            priority: task.priority,
            eval: schedule::evaluate(&input, now, &tz),
        })
    }

    /// Ranked statuses of the tasks `viewer` may see. Inactive (suggested)
    /// tasks are included only when asked.
    pub fn ranked_tasks(
        &self,
        viewer: MemberId,
        now: Timestamp,
        include_inactive: bool,
    ) -> Result<Vec<(Task, TaskStatus)>> {
        let tasks = self.visible_tasks(viewer)?;
        let mut out = Vec::new();
        for t in tasks {
            if t.active || include_inactive {
                let s = self.task_status(&t, now)?;
                out.push((t, s));
            }
        }
        let mut statuses: Vec<TaskStatus> = out.iter().map(|(_, s)| s.clone()).collect();
        priority::rank(&mut statuses);
        let mut ranked = Vec::new();
        for s in statuses {
            if let Some(pos) = out.iter().position(|(t, _)| t.id == s.task) {
                ranked.push(out.swap_remove(pos));
            }
        }
        Ok(ranked)
    }

    /// Tasks a member may see (D-23): owners and members see everything;
    /// limited members see only tasks they own.
    pub fn visible_tasks(&self, viewer: MemberId) -> Result<Vec<Task>> {
        let member: Member = self.fetch(&viewer.to_string())?;
        let tasks: Vec<Task> = self.list()?;
        Ok(match member.role {
            Role::Owner | Role::Member => tasks,
            Role::Limited => tasks.into_iter().filter(|t| t.owner == viewer).collect(),
        })
    }

    /// One-tap completion (P1): only the task is needed; notes are optional.
    pub fn complete_task(
        &mut self,
        task: TaskId,
        at: Option<Timestamp>,
        notes: Option<&str>,
    ) -> Result<Completion> {
        let _: Task = self.fetch(&task.to_string())?;
        let c = Completion {
            id: CompletionId::new(),
            task,
            by: self.author()?,
            at: at.unwrap_or_else(|| self.now()),
            notes: notes.map(|n| Flagged::normal(n.to_string())),
            cost: None,
            values: Default::default(),
            contractor: None,
            attachments: vec![],
            voids: None,
        };
        self.append(&c)?;
        Ok(c)
    }

    /// Cancel a completion logged by mistake, by appending a voiding record.
    pub fn void_completion(
        &mut self,
        completion: CompletionId,
        reason: Option<&str>,
    ) -> Result<Completion> {
        let original: Completion = self.fetch(&completion.to_string())?;
        if original.voids.is_some() {
            return Err(Error::Invalid("cannot void a voiding record".into()));
        }
        let c = Completion {
            id: CompletionId::new(),
            task: original.task,
            by: self.author()?,
            at: self.now(),
            notes: reason.map(|n| Flagged::normal(n.to_string())),
            cost: None,
            values: Default::default(),
            contractor: None,
            attachments: vec![],
            voids: Some(completion),
        };
        self.append(&c)?;
        Ok(c)
    }

    /// Snooze = reschedule to a date (P7).
    pub fn snooze_task(
        &mut self,
        task: TaskId,
        until: Date,
        reason: Option<&str>,
    ) -> Result<Snooze> {
        let _: Task = self.fetch(&task.to_string())?;
        let s = Snooze {
            id: SnoozeId::new(),
            task,
            by: self.author()?,
            at: self.now(),
            until,
            reason: reason.map(str::to_string),
        };
        self.append(&s)?;
        Ok(s)
    }

    pub fn record_reading(
        &mut self,
        meter: MeterId,
        value: f64,
        at: Option<Timestamp>,
    ) -> Result<MeterReading> {
        if !value.is_finite() {
            return Err(Error::Invalid("reading must be a number".into()));
        }
        let _: Meter = self.fetch(&meter.to_string())?;
        let r = MeterReading {
            id: MeterReadingId::new(),
            meter,
            at: at.unwrap_or_else(|| self.now()),
            value,
            by: self.author()?,
        };
        self.append(&r)?;
        Ok(r)
    }

    /// Add an item and the suggested plan from a resolved template. Stores a
    /// copy of the template so the database stands alone. Tasks are owned by
    /// `owner` and start inactive (P3).
    pub fn add_item_from_template(
        &mut self,
        item: &Item,
        template: &Template,
        owner: MemberId,
        start: Date,
    ) -> Result<Vec<Task>> {
        let (meters, tasks) = template.plan_for_item(item, owner, start);
        self.put(template)?;
        self.put(item)?;
        for m in &meters {
            self.put(m)?;
        }
        for t in &tasks {
            self.put(t)?;
        }
        Ok(tasks)
    }

    /// P3: switch on the top `n` suggested tasks. Returns the ones switched on.
    pub fn start_small(&mut self, n: usize, now: Timestamp) -> Result<Vec<TaskId>> {
        let suggested: Vec<Task> = self
            .list::<Task>()?
            .into_iter()
            .filter(|t| !t.active)
            .collect();
        let mut statuses = Vec::new();
        for t in &suggested {
            statuses.push(self.task_status(t, now)?);
        }
        let picked = priority::start_small(&statuses, n);
        for id in &picked {
            let mut t: Task = self.fetch(&id.to_string())?;
            t.active = true;
            self.put(&t)?;
        }
        Ok(picked)
    }
}
