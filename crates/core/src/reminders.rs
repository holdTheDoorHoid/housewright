//! Reminder planning (P5, D-18, D-59).
//!
//! Pure: given the ranked task statuses, one person's reminder preferences,
//! "now" and the time zone, work out which local notifications to schedule.
//! Every device runs this and hands the result to its platform scheduler
//! (Android alarms, the browser's Notifications API while the page is open).
//!
//! The plan is *stateless*: it lists reminders strictly in the future, so
//! recomputing it after any change (a completion, a snooze, a reboot) never
//! repeats a reminder that has already fired.
//!
//! Rules:
//! - one reminder on the day a task becomes due, and at most one follow-up a
//!   few days later while it is still inside its grace period (P5: "one
//!   reminder at the due time, at most one follow-up");
//! - a task snoozed to a date is reminded once on that date;
//! - reminders land in the person's maintenance window when they have one
//!   (the first allowed day on or after the date, at the window's start),
//!   otherwise at [`DEFAULT_REMINDER_TIME`];
//! - nothing fires inside quiet hours: a time that falls in them moves to the
//!   end of the quiet period.

use jiff::civil::{Date, Time, Weekday};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

use crate::ids::TaskId;
use crate::priority::TaskStatus;
use crate::schedule::{TaskState, WeekdayName};

/// Time of day used when a person has no maintenance window.
pub const DEFAULT_REMINDER_TIME: Time = Time::constant(9, 0, 0, 0);
/// How far ahead the plan looks.
pub const DEFAULT_HORIZON_DAYS: i64 = 60;
/// Most reminders one plan holds (platform schedulers have limits; Android
/// allows a few hundred alarms per app).
pub const MAX_PLANNED: usize = 64;

/// A daily span of clock time. If `end` is earlier than `start` the span
/// crosses midnight (22:00 to 07:00).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Time,
    pub end: Time,
}

impl TimeRange {
    /// Whether a clock time falls inside the span (start inclusive, end exclusive).
    pub fn contains(&self, t: Time) -> bool {
        if self.start <= self.end {
            t >= self.start && t < self.end
        } else {
            t >= self.start || t < self.end
        }
    }
}

/// A preferred time for doing maintenance ("Saturday mornings").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    /// Allowed days; empty means every day.
    #[serde(default)]
    pub days: Vec<WeekdayName>,
    pub start: Time,
    pub end: Time,
}

/// One person's reminder preferences, stored on their [`crate::model::Member`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours: Option<TimeRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_window: Option<MaintenanceWindow>,
}

/// Why a reminder is scheduled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    /// The day the task becomes due.
    Due,
    /// The single follow-up.
    FollowUp,
    /// The date the person rescheduled it to.
    Rescheduled,
}

/// The words of a reminder, supplied by the caller for each task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReminderText {
    pub task: TaskId,
    /// Item name, if the task is about an item.
    pub item: Option<String>,
    /// Where the item is (room name).
    pub place: Option<String>,
    pub why: Option<String>,
    pub minutes: Option<u32>,
}

/// One planned local notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedReminder {
    pub task: TaskId,
    pub kind: ReminderKind,
    pub at: Timestamp,
    pub title: String,
    pub body: String,
}

/// Inputs to [`plan`].
pub struct PlanInput<'a> {
    pub statuses: &'a [TaskStatus],
    pub texts: &'a [ReminderText],
    pub prefs: &'a ReminderPrefs,
    pub now: Timestamp,
    pub tz: &'a TimeZone,
    pub horizon_days: i64,
}

fn add_days(d: Date, n: i64) -> Option<Date> {
    d.checked_add(Span::new().try_days(n).ok()?).ok()
}

fn weekday_name(w: Weekday) -> WeekdayName {
    match w {
        Weekday::Monday => WeekdayName::Monday,
        Weekday::Tuesday => WeekdayName::Tuesday,
        Weekday::Wednesday => WeekdayName::Wednesday,
        Weekday::Thursday => WeekdayName::Thursday,
        Weekday::Friday => WeekdayName::Friday,
        Weekday::Saturday => WeekdayName::Saturday,
        Weekday::Sunday => WeekdayName::Sunday,
    }
}

/// The instant a reminder for `date` should fire for this person: in their
/// maintenance window if they have one, never in quiet hours.
pub fn place(date: Date, prefs: &ReminderPrefs, tz: &TimeZone) -> Option<Timestamp> {
    let (mut day, mut time) = (date, DEFAULT_REMINDER_TIME);
    if let Some(w) = &prefs.maintenance_window {
        time = w.start;
        if !w.days.is_empty() {
            // The first allowed day on or after `date` (at most a week away).
            day = (0..7)
                .filter_map(|k| add_days(date, k))
                .find(|d| w.days.contains(&weekday_name(d.weekday())))?;
        }
    }
    if let Some(q) = &prefs.quiet_hours
        && q.contains(time)
    {
        // Move to the end of the quiet period; if the period crosses midnight
        // and we are before midnight, that is the next day.
        if q.start > q.end && time >= q.start {
            day = add_days(day, 1)?;
        }
        time = q.end;
    }
    day.to_datetime(time)
        .to_zoned_datetime(tz.clone())
        .ok()
        .map(|z| z.timestamp())
}

/// Days after the due date for the single follow-up: half the grace period,
/// at least two days and at most a week.
fn follow_up_days(due: Date, overdue_after: Date) -> Option<i64> {
    let grace = due.duration_until(overdue_after).as_secs() / 86_400;
    let k = (grace / 2).clamp(2, 7);
    if k > grace { None } else { Some(k) }
}

fn compose(
    status: &TaskStatus,
    text: Option<&ReminderText>,
    kind: ReminderKind,
) -> (String, String) {
    let title = status.title.clone();
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = text {
        match (&t.item, &t.place) {
            (Some(i), Some(p)) => parts.push(format!("{i}, {p}.")),
            (Some(i), None) => parts.push(format!("{i}.")),
            _ => {}
        }
        if let Some(m) = t.minutes {
            parts.push(format!("About {m} min."));
        }
        if let Some(w) = &t.why {
            parts.push(w.clone());
        }
    }
    // Vary the opening so repeated reminders do not read identically (P5).
    let lead = match kind {
        ReminderKind::Due => "Due now.",
        ReminderKind::FollowUp => "Still to do when you get a moment.",
        ReminderKind::Rescheduled => "You picked today for this.",
    };
    parts.insert(0, lead.to_string());
    (title, parts.join(" "))
}

/// Work out the reminders to schedule, soonest first.
pub fn plan(input: &PlanInput<'_>) -> Vec<PlannedReminder> {
    let today = crate::schedule::civil_date(input.now, input.tz);
    let Some(horizon) = add_days(today, input.horizon_days) else {
        return vec![];
    };
    let mut out = Vec::new();
    for s in input.statuses {
        if !s.eval.state.is_live() {
            continue;
        }
        let Some(w) = s.eval.window else { continue };
        let text = input.texts.iter().find(|t| t.task == s.task);
        let mut candidates: Vec<(Date, ReminderKind)> = Vec::new();
        if s.eval.state == TaskState::Snoozed || (s.eval.snooze_count > 0 && w.earliest == w.due) {
            candidates.push((w.due, ReminderKind::Rescheduled));
        } else {
            candidates.push((w.due, ReminderKind::Due));
            if let Some(k) = follow_up_days(w.due, w.overdue_after)
                && let Some(d) = add_days(w.due, k)
            {
                candidates.push((d, ReminderKind::FollowUp));
            }
        }
        for (date, kind) in candidates {
            if date > horizon {
                continue;
            }
            let Some(at) = place(date, input.prefs, input.tz) else {
                continue;
            };
            if at <= input.now {
                continue;
            }
            let (title, body) = compose(s, text, kind);
            out.push(PlannedReminder {
                task: s.task,
                kind,
                at,
                title,
                body,
            });
        }
    }
    out.sort_by(|a, b| a.at.cmp(&b.at).then(a.task.cmp(&b.task)));
    out.truncate(MAX_PLANNED);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::priority::PriorityClass;
    use crate::schedule::{DueWindow, Evaluation};
    use jiff::civil::{date, time};

    fn ny() -> TimeZone {
        TimeZone::get("America/New_York").unwrap()
    }

    fn local(d: Date, t: Time) -> Timestamp {
        d.to_datetime(t)
            .to_zoned_datetime(ny())
            .unwrap()
            .timestamp()
    }

    fn status(state: TaskState, due: Date, grace: i64, snoozes: u32) -> TaskStatus {
        TaskStatus {
            task: TaskId::new(),
            title: "Test the smoke alarm".into(),
            priority: PriorityClass::LifeSafety,
            eval: Evaluation {
                state,
                window: Some(DueWindow {
                    earliest: if snoozes > 0 {
                        due
                    } else {
                        add_days(due, -5).unwrap()
                    },
                    due,
                    overdue_after: add_days(due, grace).unwrap(),
                }),
                last_done: None,
                snooze_count: snoozes,
                needs_review: false,
                usage: None,
            },
        }
    }

    fn run(statuses: &[TaskStatus], prefs: &ReminderPrefs, now: Timestamp) -> Vec<PlannedReminder> {
        let tz = ny();
        plan(&PlanInput {
            statuses,
            texts: &[],
            prefs,
            now,
            tz: &tz,
            horizon_days: DEFAULT_HORIZON_DAYS,
        })
    }

    #[test]
    fn due_and_one_follow_up_at_nine() {
        let s = status(TaskState::Scheduled, date(2026, 12, 10), 30, 0);
        let now = local(date(2026, 11, 27), time(12, 0, 0, 0));
        let r = run(&[s], &ReminderPrefs::default(), now);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].kind, ReminderKind::Due);
        assert_eq!(r[0].at, local(date(2026, 12, 10), time(9, 0, 0, 0)));
        assert_eq!(r[1].kind, ReminderKind::FollowUp);
        assert_eq!(r[1].at, local(date(2026, 12, 17), time(9, 0, 0, 0)));
    }

    #[test]
    fn past_reminders_are_never_repeated() {
        let s = status(TaskState::Due, date(2026, 11, 20), 30, 0);
        let now = local(date(2026, 11, 27), time(12, 0, 0, 0));
        // Due reminder (Nov 20) and follow-up (Nov 27 09:00) are both past.
        assert!(run(&[s], &ReminderPrefs::default(), now).is_empty());
    }

    #[test]
    fn short_grace_has_no_follow_up() {
        let s = status(TaskState::Scheduled, date(2026, 12, 10), 1, 0);
        let now = local(date(2026, 11, 27), time(12, 0, 0, 0));
        assert_eq!(run(&[s], &ReminderPrefs::default(), now).len(), 1);
    }

    #[test]
    fn snoozed_task_is_reminded_once_on_its_date() {
        let s = status(TaskState::Snoozed, date(2026, 12, 5), 30, 1);
        let now = local(date(2026, 11, 27), time(12, 0, 0, 0));
        let r = run(&[s], &ReminderPrefs::default(), now);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, ReminderKind::Rescheduled);
    }

    #[test]
    fn maintenance_window_moves_to_allowed_day() {
        let prefs = ReminderPrefs {
            quiet_hours: None,
            maintenance_window: Some(MaintenanceWindow {
                days: vec![WeekdayName::Saturday],
                start: time(10, 30, 0, 0),
                end: time(12, 0, 0, 0),
            }),
        };
        // 2026-12-10 is a Thursday; the next Saturday is the 12th.
        let at = place(date(2026, 12, 10), &prefs, &ny()).unwrap();
        assert_eq!(at, local(date(2026, 12, 12), time(10, 30, 0, 0)));
    }

    #[test]
    fn quiet_hours_push_to_their_end() {
        let prefs = ReminderPrefs {
            quiet_hours: Some(TimeRange {
                start: time(22, 0, 0, 0),
                end: time(9, 30, 0, 0),
            }),
            maintenance_window: None,
        };
        let at = place(date(2026, 12, 10), &prefs, &ny()).unwrap();
        assert_eq!(at, local(date(2026, 12, 10), time(9, 30, 0, 0)));

        // A window starting inside late-evening quiet hours moves to the next morning.
        let prefs = ReminderPrefs {
            quiet_hours: Some(TimeRange {
                start: time(21, 0, 0, 0),
                end: time(7, 0, 0, 0),
            }),
            maintenance_window: Some(MaintenanceWindow {
                days: vec![],
                start: time(22, 0, 0, 0),
                end: time(23, 0, 0, 0),
            }),
        };
        let at = place(date(2026, 12, 10), &prefs, &ny()).unwrap();
        assert_eq!(at, local(date(2026, 12, 11), time(7, 0, 0, 0)));
    }

    #[test]
    fn time_range_contains() {
        let night = TimeRange {
            start: time(22, 0, 0, 0),
            end: time(7, 0, 0, 0),
        };
        assert!(night.contains(time(23, 0, 0, 0)));
        assert!(night.contains(time(3, 0, 0, 0)));
        assert!(!night.contains(time(7, 0, 0, 0)));
        assert!(!night.contains(time(12, 0, 0, 0)));
    }

    #[test]
    fn paused_and_completed_are_not_reminded() {
        let now = local(date(2026, 11, 27), time(12, 0, 0, 0));
        for st in [
            TaskState::Paused,
            TaskState::NotApplicable,
            TaskState::Completed,
        ] {
            let s = status(st, date(2026, 12, 10), 30, 0);
            assert!(run(&[s], &ReminderPrefs::default(), now).is_empty());
        }
    }
}
