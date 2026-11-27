//! The schedule engine (D-20). Pure: no clock, no I/O, no database.
//!
//! Given a task's schedule, its completion and snooze history, meter readings,
//! the property's climate, the property's time zone and "now", [`evaluate`]
//! returns the current due *window* and a state. The same inputs give the same
//! answer on every device.
//!
//! ## Why `jiff`
//!
//! We use `jiff` rather than `chrono` + `chrono-tz` because it has one coherent
//! model for civil dates, time zones and spans: `Date + 1 month` clamps
//! Jan 31 to Feb 28/29 by design, time zones come from the system tz database
//! (or a bundled copy where none exists, such as WebAssembly) without a second
//! crate, and its 1.x API is committed to stability. It is MIT/Unlicense.
//!
//! ## DST safety
//!
//! All arithmetic is on civil dates (`jiff::civil::Date`), never on instants.
//! Instants (completion times, "now") are converted to a civil date in the
//! property's time zone exactly once, at the edge. A day is a calendar day, so a
//! 23- or 25-hour day around a clock change cannot shift a due date.
//!
//! ## Windows and states
//!
//! A due window is `earliest <= due <= overdue_after`. Doing the task on or
//! after `earliest` counts for that occurrence.
//!
//! | today                          | state      |
//! |--------------------------------|------------|
//! | before `earliest`              | scheduled  |
//! | `earliest` up to the day before `due` | due soon |
//! | `due` through `overdue_after`  | due        |
//! | after `overdue_after`          | overdue    |
//!
//! Overriding states, in order: not applicable, paused, completed (a one-off
//! that has been done), snoozed.

use std::fmt;
use std::str::FromStr;

use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

/// After this many snoozes since the last completion the engine flags the task
/// for review (P7: change the interval, hand it on, mark it not applicable, or
/// find a pro).
pub const SNOOZE_REVIEW_THRESHOLD: u32 = 3;

/// A usage task becomes "due soon" at this fraction of its threshold.
pub const USAGE_DUE_SOON_FRACTION: f64 = 0.9;

// ---------------------------------------------------------------------------
// Intervals
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

/// "Every N units". Written in files as `"3 months"`, `"1 year"`, `"90 days"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Interval {
    pub n: u32,
    pub unit: Unit,
}

impl Interval {
    pub const fn new(n: u32, unit: Unit) -> Self {
        Self { n, unit }
    }
    pub const fn days(n: u32) -> Self {
        Self::new(n, Unit::Day)
    }
    pub const fn weeks(n: u32) -> Self {
        Self::new(n, Unit::Week)
    }
    pub const fn months(n: u32) -> Self {
        Self::new(n, Unit::Month)
    }
    pub const fn years(n: u32) -> Self {
        Self::new(n, Unit::Year)
    }

    /// `date + k * self`, computed from `date` in one step so that repeated
    /// occurrences never drift (Jan 31 + 2 months is Mar 31, not Mar 28/29).
    /// Month-end overflow clamps (Jan 31 + 1 month = Feb 28 or 29).
    pub fn add_times(self, date: Date, k: i64) -> Option<Date> {
        let n = i64::from(self.n).checked_mul(k)?;
        let span = match self.unit {
            Unit::Day => Span::new().try_days(n).ok()?,
            Unit::Week => Span::new().try_days(n.checked_mul(7)?).ok()?,
            Unit::Month => Span::new().try_months(n).ok()?,
            Unit::Year => Span::new().try_years(n).ok()?,
        };
        date.checked_add(span).ok()
    }

    pub fn add_to(self, date: Date) -> Option<Date> {
        self.add_times(date, 1)
    }

    /// Typical length in days (for default windows).
    pub fn approx_days(self) -> u32 {
        let per = match self.unit {
            Unit::Day => 1,
            Unit::Week => 7,
            Unit::Month => 30,
            Unit::Year => 365,
        };
        self.n.saturating_mul(per)
    }

    /// Longest possible length in days (for safe search starting points).
    fn max_days(self) -> i64 {
        let per = match self.unit {
            Unit::Day => 1,
            Unit::Week => 7,
            Unit::Month => 31,
            Unit::Year => 366,
        };
        i64::from(self.n) * per
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self.unit {
            Unit::Day => "day",
            Unit::Week => "week",
            Unit::Month => "month",
            Unit::Year => "year",
        };
        if self.n == 1 {
            write!(f, "1 {unit}")
        } else {
            write!(f, "{} {unit}s", self.n)
        }
    }
}

impl FromStr for Interval {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let mut parts = s.split_whitespace();
        let (Some(n), Some(unit), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(format!("interval '{s}' should look like '3 months'"));
        };
        let n: u32 = n
            .parse()
            .map_err(|_| format!("interval '{s}': '{n}' is not a whole number"))?;
        if n == 0 {
            return Err(format!("interval '{s}' must be at least 1"));
        }
        let unit = match unit.to_ascii_lowercase().trim_end_matches('s') {
            "day" => Unit::Day,
            "week" => Unit::Week,
            "month" => Unit::Month,
            "year" => Unit::Year,
            other => return Err(format!("interval '{s}': unknown unit '{other}'")),
        };
        Ok(Self { n, unit })
    }
}

impl TryFrom<String> for Interval {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<Interval> for String {
    fn from(i: Interval) -> String {
        i.to_string()
    }
}

// ---------------------------------------------------------------------------
// Climate and seasons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hemisphere {
    #[default]
    North,
    South,
}

/// The climate inputs seasonal schedules need. Phase 1 takes them as given; a
/// later phase derives them from a zip code.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Climate {
    #[serde(default)]
    pub hemisphere: Hemisphere,
    /// Average first autumn frost, as a day of year in a non-leap year (1-365).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_frost_doy: Option<u16>,
    /// Average last spring frost, as a day of year in a non-leap year (1-365).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frost_doy: Option<u16>,
}

/// Named seasonal windows. Meteorological seasons (whole months), flipped for
/// the southern hemisphere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Spring,
    Summer,
    #[serde(alias = "fall")]
    Autumn,
    Winter,
    /// Six weeks before the average first frost, up to the day before it.
    /// Without frost data: September-October (north), March-April (south).
    BeforeFirstFrost,
    /// From the day after the average last frost, for six weeks.
    /// Without frost data: April-May (north), October-November (south).
    AfterLastFrost,
}

/// Days of lead time before a frost date that a "before first frost" window opens.
const FROST_WINDOW_DAYS: i64 = 42;

/// Convert a non-leap day-of-year to a date in `year`. Using a fixed non-leap
/// calendar means "day 288" is always Oct 15, leap year or not.
fn doy_to_date(year: i16, doy: u16) -> Option<Date> {
    let doy = doy.clamp(1, 365);
    let base = Date::new(2025, 1, 1).ok()?;
    let d = base
        .checked_add(Span::new().try_days(i64::from(doy) - 1).ok()?)
        .ok()?;
    Date::new(year, d.month(), d.day()).ok()
}

fn ymd(y: i16, m: i8, d: i8) -> Option<Date> {
    Date::new(y, m, d).ok()
}

fn month_end(y: i16, m: i8) -> Option<Date> {
    Some(ymd(y, m, 1)?.last_of_month())
}

/// The inclusive date range of `season` that starts in `year`.
pub fn season_range(season: Season, climate: &Climate, year: i16) -> Option<(Date, Date)> {
    use Hemisphere::*;
    use Season::*;
    let h = climate.hemisphere;
    let months = |a: i8, b: i8| -> Option<(Date, Date)> {
        // b < a means the range runs into the next year (northern winter).
        let end_year = if b < a { year.checked_add(1)? } else { year };
        Some((ymd(year, a, 1)?, month_end(end_year, b)?))
    };
    match (season, h) {
        (Spring, North) | (Autumn, South) => months(3, 5),
        (Summer, North) | (Winter, South) => months(6, 8),
        (Autumn, North) | (Spring, South) => months(9, 11),
        (Winter, North) | (Summer, South) => months(12, 2),
        (BeforeFirstFrost, _) => match climate.first_frost_doy {
            Some(doy) => {
                let f = doy_to_date(year, doy)?;
                Some((add_days(f, -FROST_WINDOW_DAYS)?, add_days(f, -1)?))
            }
            None if h == North => months(9, 10),
            None => months(3, 4),
        },
        (AfterLastFrost, _) => match climate.last_frost_doy {
            Some(doy) => {
                let l = doy_to_date(year, doy)?;
                Some((add_days(l, 1)?, add_days(l, FROST_WINDOW_DAYS)?))
            }
            None if h == North => months(4, 5),
            None => months(10, 11),
        },
    }
}

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

/// Days of the week, as written in files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekdayName {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl From<WeekdayName> for Weekday {
    fn from(w: WeekdayName) -> Weekday {
        match w {
            WeekdayName::Monday => Weekday::Monday,
            WeekdayName::Tuesday => Weekday::Tuesday,
            WeekdayName::Wednesday => Weekday::Wednesday,
            WeekdayName::Thursday => Weekday::Thursday,
            WeekdayName::Friday => Weekday::Friday,
            WeekdayName::Saturday => Weekday::Saturday,
            WeekdayName::Sunday => Weekday::Sunday,
        }
    }
}

/// All schedule kinds from D-20.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Fixed calendar: every N days/weeks/months/years from an anchor date,
    /// regardless of when it was last done. No anchor = the task's start date.
    Fixed {
        every: Interval,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Date>,
    },
    /// Fixed calendar: the nth weekday of the month (`nth` 1-4, or -1 for the
    /// last), in the listed months (empty = every month).
    NthWeekday {
        nth: i8,
        weekday: WeekdayName,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        months: Vec<u8>,
    },
    /// Fixed calendar: specific months of the year, on a day of the month
    /// (clamped to the month's length; default the 1st).
    MonthsOfYear {
        months: Vec<u8>,
        #[serde(default = "one")]
        day: u8,
    },
    /// Floating: N after the last completion.
    Floating { every: Interval },
    /// Seasonal: once per year in a named window.
    Seasonal { season: Season },
    /// Usage: when a meter has advanced `threshold` since the last completion,
    /// or after `or_every`, whichever comes first.
    Usage {
        /// Meter key on the same item (`runtime_hours`, `loads`).
        meter: String,
        threshold: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        or_every: Option<Interval>,
    },
    /// One-off: a single due date.
    OneOff { due: Date },
}

fn one() -> u8 {
    1
}

impl Schedule {
    /// Human summary, e.g. "every 3 months".
    pub fn describe(&self) -> String {
        match self {
            Schedule::Fixed { every, .. } => format!("every {every}"),
            Schedule::NthWeekday {
                nth,
                weekday,
                months,
            } => {
                let which = match nth {
                    -1 => "last".to_string(),
                    1 => "1st".into(),
                    2 => "2nd".into(),
                    3 => "3rd".into(),
                    n => format!("{n}th"),
                };
                let m = if months.is_empty() {
                    "every month".to_string()
                } else {
                    format!("in months {months:?}")
                };
                format!("{which} {weekday:?} {m}")
            }
            Schedule::MonthsOfYear { months, day } => {
                format!("day {day} of months {months:?}")
            }
            Schedule::Floating { every } => format!("{every} after last done"),
            Schedule::Seasonal { season } => format!("seasonal: {season:?}"),
            Schedule::Usage {
                meter,
                threshold,
                or_every,
            } => match or_every {
                Some(i) => format!("every {threshold} {meter} or {i}, whichever first"),
                None => format!("every {threshold} {meter}"),
            },
            Schedule::OneOff { due } => format!("once, on {due}"),
        }
    }

    /// Check the schedule's own parameters.
    pub fn validate(&self) -> Result<(), String> {
        let month_ok = |m: &u8| (1..=12).contains(m);
        match self {
            Schedule::Fixed { .. } | Schedule::Floating { .. } | Schedule::Seasonal { .. } => {}
            Schedule::NthWeekday { nth, months, .. } => {
                if !matches!(nth, -1 | 1..=4) {
                    return Err(format!("nth_weekday: nth must be 1-4 or -1, got {nth}"));
                }
                if !months.iter().all(month_ok) {
                    return Err("nth_weekday: months must be 1-12".into());
                }
            }
            Schedule::MonthsOfYear { months, day } => {
                if months.is_empty() || !months.iter().all(month_ok) {
                    return Err("months_of_year: list one or more months, 1-12".into());
                }
                if !(1..=31).contains(day) {
                    return Err("months_of_year: day must be 1-31".into());
                }
            }
            Schedule::Usage {
                meter, threshold, ..
            } => {
                if meter.trim().is_empty() {
                    return Err("usage: meter key is empty".into());
                }
                if !(threshold.is_finite() && *threshold > 0.0) {
                    return Err("usage: threshold must be a positive number".into());
                }
            }
            Schedule::OneOff { .. } => {}
        }
        Ok(())
    }

    /// The default due window for this schedule.
    pub fn default_window(&self) -> WindowPolicy {
        match self {
            Schedule::Fixed { every, .. } | Schedule::Floating { every } => {
                WindowPolicy::for_period(every.approx_days())
            }
            Schedule::NthWeekday { months, .. } => {
                let per_year = if months.is_empty() { 12 } else { months.len() };
                WindowPolicy::for_period(365 / per_year as u32)
            }
            Schedule::MonthsOfYear { months, .. } => {
                WindowPolicy::for_period(365 / months.len().max(1) as u32)
            }
            Schedule::Seasonal { .. } => WindowPolicy {
                lead_days: 14,
                grace_days: 0,
            },
            Schedule::Usage { or_every, .. } => match or_every {
                Some(i) => WindowPolicy::for_period(i.approx_days()),
                None => WindowPolicy {
                    lead_days: 7,
                    grace_days: 14,
                },
            },
            Schedule::OneOff { .. } => WindowPolicy {
                lead_days: 7,
                grace_days: 7,
            },
        }
    }
}

/// How wide a due window is around its due date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPolicy {
    /// Days before `due` that the task becomes "due soon" and doing it counts.
    pub lead_days: u32,
    /// Days after `due` before it is "overdue". For seasonal tasks, days after
    /// the end of the season.
    pub grace_days: u32,
}

impl WindowPolicy {
    /// A sixth of the period each side, capped at two weeks of lead and a
    /// month of grace. Monthly: 5/5. Quarterly: 14/15. Yearly: 14/30.
    pub fn for_period(days: u32) -> Self {
        Self {
            lead_days: (days / 6).min(14),
            grace_days: (days / 6).min(30),
        }
    }

    fn around(self, due: Date) -> Option<DueWindow> {
        Some(DueWindow {
            earliest: add_days(due, -i64::from(self.lead_days))?,
            due,
            overdue_after: add_days(due, i64::from(self.grace_days))?,
        })
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// A due window: doing the task on any day from `earliest` counts; it is due
/// from `due`; it is overdue after `overdue_after`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DueWindow {
    pub earliest: Date,
    pub due: Date,
    pub overdue_after: Date,
}

impl DueWindow {
    fn state_on(&self, today: Date) -> TaskState {
        if today < self.earliest {
            TaskState::Scheduled
        } else if today < self.due {
            TaskState::DueSoon
        } else if today <= self.overdue_after {
            TaskState::Due
        } else {
            TaskState::Overdue
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Scheduled,
    DueSoon,
    Due,
    Overdue,
    Snoozed,
    Paused,
    NotApplicable,
    /// A one-off task that has been done. (Not in the original state list;
    /// see D-54.)
    Completed,
}

impl TaskState {
    /// Higher is more urgent. Used for ranking and combining usage and calendar.
    pub fn urgency(self) -> u8 {
        match self {
            TaskState::Overdue => 6,
            TaskState::Due => 5,
            TaskState::DueSoon => 4,
            TaskState::Snoozed => 3,
            TaskState::Scheduled => 2,
            TaskState::Paused => 1,
            TaskState::NotApplicable | TaskState::Completed => 0,
        }
    }

    /// Whether the task belongs on an active to-do list.
    pub fn is_live(self) -> bool {
        !matches!(
            self,
            TaskState::Paused | TaskState::NotApplicable | TaskState::Completed
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            TaskState::Scheduled => "scheduled",
            TaskState::DueSoon => "due soon",
            TaskState::Due => "due",
            TaskState::Overdue => "overdue",
            TaskState::Snoozed => "snoozed",
            TaskState::Paused => "paused",
            TaskState::NotApplicable => "not applicable",
            TaskState::Completed => "completed",
        }
    }
}

/// A snooze, as the engine sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnoozeInput {
    pub at: Timestamp,
    pub until: Date,
}

/// A meter reading, as the engine sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadingInput {
    pub at: Timestamp,
    pub value: f64,
}

/// Everything the engine needs about one task.
#[derive(Clone, Debug)]
pub struct ScheduleInput<'a> {
    pub schedule: &'a Schedule,
    pub window: Option<WindowPolicy>,
    pub start: Date,
    /// Times of (non-voided) completions, any order.
    pub completions: &'a [Timestamp],
    pub snoozes: &'a [SnoozeInput],
    /// Readings of the meter a usage schedule refers to, any order.
    pub readings: &'a [ReadingInput],
    pub climate: &'a Climate,
    pub paused: bool,
    pub not_applicable: bool,
}

/// Usage progress since the last completion.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageProgress {
    pub used: f64,
    pub threshold: f64,
}

/// The engine's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    pub state: TaskState,
    /// None for paused/not applicable tasks, and for usage tasks with too few
    /// readings to project a date.
    pub window: Option<DueWindow>,
    pub last_done: Option<Date>,
    /// Snoozes since the last completion.
    pub snooze_count: u32,
    /// P7: snoozed too often; ask the person what to do with it.
    pub needs_review: bool,
    pub usage: Option<UsageProgress>,
}

/// The civil date of an instant in a time zone.
pub fn civil_date(at: Timestamp, tz: &TimeZone) -> Date {
    tz.to_datetime(at).date()
}

fn add_days(d: Date, n: i64) -> Option<Date> {
    d.checked_add(Span::new().try_days(n).ok()?).ok()
}

fn days_between(a: Date, b: Date) -> i64 {
    a.duration_until(b).as_secs() / 86_400
}

/// Evaluate one task. Pure: `now` and `tz` are inputs.
pub fn evaluate(input: &ScheduleInput<'_>, now: Timestamp, tz: &TimeZone) -> Evaluation {
    let today = civil_date(now, tz);
    let last_done_at = input.completions.iter().copied().max();
    let last_done = last_done_at.map(|t| civil_date(t, tz));
    let mut eval = Evaluation {
        state: TaskState::Scheduled,
        window: None,
        last_done,
        snooze_count: 0,
        needs_review: false,
        usage: None,
    };
    if input.not_applicable {
        eval.state = TaskState::NotApplicable;
        return eval;
    }
    if input.paused {
        eval.state = TaskState::Paused;
        return eval;
    }

    let policy = input
        .window
        .unwrap_or_else(|| input.schedule.default_window());

    let (state, window) = match input.schedule {
        Schedule::OneOff { due } => {
            if last_done.is_some() {
                eval.state = TaskState::Completed;
                eval.window = policy.around(*due);
                return eval;
            }
            by_date(policy.around(*due), today)
        }
        Schedule::Floating { every } => {
            let due = match last_done {
                Some(l) => every.add_to(l),
                None => Some(input.start),
            };
            by_date(due.and_then(|d| policy.around(d)), today)
        }
        Schedule::Usage {
            threshold,
            or_every,
            ..
        } => {
            let reference = last_done.unwrap_or(input.start);
            let (u_state, u_window, progress) =
                usage_eval(input.readings, *threshold, reference, policy, today, tz);
            eval.usage = progress;
            let usage_side = (u_state, u_window);
            match or_every {
                Some(i) => {
                    let cal = by_date(i.add_to(reference).and_then(|d| policy.around(d)), today);
                    more_urgent(usage_side, cal)
                }
                None => usage_side,
            }
        }
        periodic => {
            let w = select_periodic(periodic, input, policy, last_done);
            by_date(w, today)
        }
    };
    eval.state = state;
    eval.window = window;

    // Snoozes since the last completion (P7).
    let relevant: Vec<&SnoozeInput> = input
        .snoozes
        .iter()
        .filter(|s| last_done_at.is_none_or(|l| s.at > l))
        .collect();
    eval.snooze_count = relevant.len() as u32;
    eval.needs_review = eval.snooze_count >= SNOOZE_REVIEW_THRESHOLD;
    if let Some(latest) = relevant.iter().max_by_key(|s| s.at) {
        let until = latest.until;
        let pushes_later = eval.window.is_none_or(|w| until > w.due);
        if pushes_later {
            let w = DueWindow {
                earliest: until,
                due: until,
                overdue_after: add_days(until, i64::from(policy.grace_days)).unwrap_or(until),
            };
            eval.window = Some(w);
            eval.state = if today < until {
                TaskState::Snoozed
            } else {
                w.state_on(today)
            };
        }
    }
    eval
}

fn by_date(window: Option<DueWindow>, today: Date) -> (TaskState, Option<DueWindow>) {
    match window {
        Some(w) => (w.state_on(today), Some(w)),
        None => (TaskState::Scheduled, None),
    }
}

fn more_urgent(
    a: (TaskState, Option<DueWindow>),
    b: (TaskState, Option<DueWindow>),
) -> (TaskState, Option<DueWindow>) {
    use std::cmp::Ordering::*;
    match a.0.urgency().cmp(&b.0.urgency()) {
        Greater => a,
        Less => b,
        Equal => match (a.1, b.1) {
            (Some(wa), Some(wb)) => {
                if wa.due <= wb.due {
                    a
                } else {
                    b
                }
            }
            (None, Some(_)) => b,
            _ => a,
        },
    }
}

/// Usage: compare meter advance since the reference date with the threshold.
fn usage_eval(
    readings: &[ReadingInput],
    threshold: f64,
    reference: Date,
    policy: WindowPolicy,
    today: Date,
    tz: &TimeZone,
) -> (TaskState, Option<DueWindow>, Option<UsageProgress>) {
    let mut rs: Vec<(Date, Timestamp, f64)> = readings
        .iter()
        .filter(|r| r.value.is_finite())
        .map(|r| (civil_date(r.at, tz), r.at, r.value))
        .collect();
    rs.sort_by(|a, b| a.1.cmp(&b.1));
    if rs.is_empty() {
        return (TaskState::Scheduled, None, None);
    }
    // Baseline: the latest reading on or before the reference date, else the
    // first reading after it.
    let base_idx = rs
        .iter()
        .rposition(|r| r.0 <= reference)
        .unwrap_or_default();
    let (base_date, _, base_value) = rs[base_idx];
    let after = &rs[base_idx..];
    let (last_date, _, last_value) = *after.last().expect("non-empty");
    let used = (last_value - base_value).max(0.0);
    let progress = Some(UsageProgress { used, threshold });

    if let Some(cross) = after.iter().find(|r| r.2 - base_value >= threshold) {
        let w = policy.around(cross.0);
        let (mut s, w) = by_date(w, today);
        if s.urgency() < TaskState::Due.urgency() {
            s = TaskState::Due;
        }
        return (s, w, progress);
    }

    // Not crossed yet: project from the average rate, if we can.
    let span_days = days_between(base_date, last_date);
    let projected = if span_days > 0 && used > 0.0 {
        let rate = used / span_days as f64;
        let need = ((threshold - used) / rate).ceil();
        if need.is_finite() && need < 3650.0 {
            add_days(last_date, need as i64)
        } else {
            None
        }
    } else {
        None
    };
    let (mut s, w) = by_date(projected.and_then(|d| policy.around(d)), today);
    // Without the meter actually reaching the threshold, the most it can be is
    // "due soon".
    if s.urgency() > TaskState::DueSoon.urgency() {
        s = TaskState::DueSoon;
    }
    if used >= threshold * USAGE_DUE_SOON_FRACTION {
        s = TaskState::DueSoon;
    }
    (s, w, progress)
}

/// For calendar and seasonal schedules: the window that is currently owed.
///
/// With a completion on date L: the first window that opened after L (a
/// completion on or after a window's `earliest` satisfies it and every earlier
/// one). Without one: the first window that is not over before the task's start.
fn select_periodic(
    schedule: &Schedule,
    input: &ScheduleInput<'_>,
    policy: WindowPolicy,
    last_done: Option<Date>,
) -> Option<DueWindow> {
    let wanted = |w: &DueWindow| match last_done {
        Some(l) => w.earliest > l,
        None => w.overdue_after >= input.start,
    };
    let reference = match last_done {
        Some(l) => l.min(input.start),
        None => input.start,
    };
    match schedule {
        Schedule::Fixed { every, anchor } => {
            let anchor = anchor.unwrap_or(input.start);
            let margin = i64::from(policy.lead_days) + i64::from(policy.grace_days);
            let gap = days_between(anchor, reference) - margin;
            let k0 = if gap > 0 {
                (gap / every.max_days() - 1).max(0)
            } else {
                0
            };
            // Bounded search: plenty for daily tasks over centuries.
            for k in k0..k0 + 200_000 {
                let due = every.add_times(anchor, k)?;
                let w = policy.around(due)?;
                if wanted(&w) {
                    return Some(w);
                }
            }
            None
        }
        _ => {
            let first_year = reference.year().checked_sub(1)?;
            for year in first_year..first_year.saturating_add(400) {
                let mut ws = year_windows(schedule, input.climate, policy, year);
                ws.sort_by_key(|w| w.due);
                if let Some(w) = ws.into_iter().find(|w| wanted(w)) {
                    return Some(w);
                }
            }
            None
        }
    }
}

/// Windows of a yearly-repeating schedule whose due date falls in (or, for a
/// season, starts in) `year`.
fn year_windows(
    schedule: &Schedule,
    climate: &Climate,
    policy: WindowPolicy,
    year: i16,
) -> Vec<DueWindow> {
    let mut out = Vec::new();
    match schedule {
        Schedule::NthWeekday {
            nth,
            weekday,
            months,
        } => {
            for m in 1..=12u8 {
                if !months.is_empty() && !months.contains(&m) {
                    continue;
                }
                let Some(first) = ymd(year, m as i8, 1) else {
                    continue;
                };
                if let Ok(d) = first.nth_weekday_of_month(*nth, (*weekday).into())
                    && let Some(w) = policy.around(d)
                {
                    out.push(w);
                }
            }
        }
        Schedule::MonthsOfYear { months, day } => {
            for &m in months {
                let Some(first) = ymd(year, m as i8, 1) else {
                    continue;
                };
                let d = (*day as i8).min(first.days_in_month());
                if let Some(w) = ymd(year, m as i8, d).and_then(|d| policy.around(d)) {
                    out.push(w);
                }
            }
        }
        Schedule::Seasonal { season } => {
            if let Some((start, end)) = season_range(*season, climate, year)
                && let (Some(earliest), Some(overdue_after)) = (
                    add_days(start, -i64::from(policy.lead_days)),
                    add_days(end, i64::from(policy.grace_days)),
                )
            {
                out.push(DueWindow {
                    earliest,
                    due: start,
                    overdue_after,
                });
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests;
