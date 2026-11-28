//! Argument and result types of the command surface. Their JSON shapes are
//! mirrored by hand in `app/src/lib/api/types.ts`; keep the two in step.

use std::collections::BTreeMap;

use housewright_core::Error;
use housewright_core::climate::ClimateLookup;
use housewright_core::fields::FieldDef;
use housewright_core::model::Role;
use housewright_core::priority::PriorityClass;
use housewright_core::profile::Profile;
use housewright_core::reminders::{PlannedReminder, ReminderKind, ReminderPrefs};
use housewright_core::schedule::TaskState;
use housewright_core::suggest::{Answer, Suggestion};
use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

/// An error as the interface sees it: a stable code and a sentence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// `invalid`, `not_found`, `not_initialized`, `import` or `internal`.
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid".into(),
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal".into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        let code = match &e {
            Error::NotFound(_) => "not_found",
            Error::Invalid(_) | Error::Template { .. } | Error::Time(_) => "invalid",
            Error::Import(_) | Error::Json(_) => "import",
            Error::Db(_) | Error::Io { .. } => "internal",
        };
        Self {
            code: code.into(),
            message: e.to_string(),
        }
    }
}

impl From<jiff::Error> for ApiError {
    fn from(e: jiff::Error) -> Self {
        ApiError::invalid(e.to_string())
    }
}

// -- arguments ------------------------------------------------------------

#[derive(Deserialize)]
pub struct InitArgs {
    #[serde(default)]
    pub household_name: String,
    pub property_name: String,
    pub your_name: String,
    pub time_zone: String,
}

#[derive(Deserialize)]
pub struct ImportArgs {
    pub json: String,
}

#[derive(Deserialize)]
pub struct CompleteArgs {
    pub id: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// When it was done, if not now (milliseconds since the epoch).
    #[serde(default)]
    pub at_ms: Option<i64>,
}

#[derive(Deserialize)]
pub struct AnnotateArgs {
    pub completion_id: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct SnoozeArgs {
    pub id: String,
    pub until: Date,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct ReviewArgs {
    pub id: String,
    /// `change_interval`, `reassign`, `not_applicable`, `pause` or `resume`.
    pub action: String,
    #[serde(default)]
    pub every: Option<String>,
    #[serde(default)]
    pub member: Option<String>,
}

#[derive(Deserialize)]
pub struct SetActiveArgs {
    pub id: String,
    pub active: bool,
}

#[derive(Deserialize)]
pub struct AddFromTemplateArgs {
    pub template: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub room_id: Option<String>,
    #[serde(default)]
    pub new_room: Option<String>,
    #[serde(default)]
    pub make: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub installed: Option<Date>,
    /// Field values as typed, by field key.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub struct StartSmallArgs {
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub n: Option<usize>,
}

#[derive(Deserialize)]
pub struct PrefsArgs {
    #[serde(default)]
    pub member_id: Option<String>,
    pub prefs: ReminderPrefs,
}

#[derive(Deserialize)]
pub struct PlanArgs {
    #[serde(default)]
    pub horizon_days: Option<i64>,
}

#[derive(Deserialize)]
pub struct TestReminderArgs {
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub in_seconds: Option<i64>,
}

// -- results --------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct MemberDto {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub reminders: ReminderPrefs,
}

#[derive(Clone, Debug, Serialize)]
pub struct PropertyDto {
    pub id: String,
    pub name: String,
    pub time_zone: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusOut {
    pub initialized: bool,
    pub household_name: Option<String>,
    pub me: Option<MemberDto>,
    pub property: Option<PropertyDto>,
}

/// A task as a list row.
#[derive(Clone, Debug, Serialize)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub item_id: Option<String>,
    pub item_name: Option<String>,
    pub room_name: Option<String>,
    pub state: TaskState,
    pub state_label: String,
    pub priority: PriorityClass,
    pub priority_label: String,
    pub due: Option<Date>,
    pub earliest: Option<Date>,
    pub overdue_after: Option<Date>,
    pub last_done: Option<Date>,
    pub snooze_count: u32,
    pub needs_review: bool,
    pub minutes: Option<u32>,
    pub owner_id: String,
    pub owner_name: String,
    pub active: bool,
    pub schedule_text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WelcomeBackOut {
    pub focus: Vec<Card>,
    pub others_overdue: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct TodayOut {
    /// This week's single suggestion (P3), if any.
    pub suggestion: Option<Suggestion>,
    /// Whether the home questions have been answered (or skipped) yet.
    pub has_profile: bool,
    /// Due soon, due and overdue, most important first.
    pub tasks: Vec<Card>,
    /// The next few scheduled tasks, soonest first.
    pub coming_up: Vec<Card>,
    pub lapsed: bool,
    pub welcome_back: Option<WelcomeBackOut>,
    pub active_count: usize,
    pub suggested_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompletionOut {
    pub id: String,
    pub task_id: String,
    pub task_title: String,
    pub at: Timestamp,
    pub by_name: String,
    pub notes: Vec<String>,
    pub voided: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskDetail {
    pub card: Card,
    pub steps: Vec<String>,
    pub why: Option<String>,
    pub source: Option<String>,
    /// "3 months" for fixed and floating schedules.
    pub interval: Option<String>,
    pub paused: bool,
    pub not_applicable: bool,
    pub completions: Vec<CompletionOut>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompleteOut {
    pub completion_id: String,
    pub card: Card,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnoozeOption {
    pub key: String,
    pub label: String,
    pub date: Date,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemRow {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub room_id: Option<String>,
    pub room_name: Option<String>,
    pub template: Option<String>,
    /// State of its most urgent switched-on task.
    pub state: Option<TaskState>,
    pub next_due: Option<Date>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FieldOut {
    pub key: String,
    pub label: String,
    pub value: String,
    pub sensitive: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemDetail {
    pub row: ItemRow,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub installed: Option<Date>,
    pub purchased: Option<Date>,
    pub fields: Vec<FieldOut>,
    pub tasks: Vec<Card>,
    pub history: Vec<CompletionOut>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RoomDto {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateTaskSummary {
    pub key: String,
    pub title: String,
    pub priority: PriorityClass,
    pub schedule_text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
    pub tasks: Vec<TemplateTaskSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AddedItem {
    pub item_id: String,
    pub tasks: Vec<Card>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntry {
    pub at: Timestamp,
    pub author: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReminderOut {
    pub task_id: String,
    pub kind: ReminderKind,
    pub at_ms: i64,
    pub title: String,
    pub body: String,
}

impl From<PlannedReminder> for ReminderOut {
    fn from(r: PlannedReminder) -> Self {
        Self {
            task_id: r.task.to_string(),
            kind: r.kind,
            at_ms: r.at.as_millisecond(),
            title: r.title,
            body: r.body,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExportOut {
    pub filename: String,
    pub json: String,
}

#[derive(Deserialize)]
pub struct ZipArgs {
    pub zip: String,
}

#[derive(Deserialize)]
pub struct OnboardArgs {
    /// The answers. `id` may be left out: it is always the property's id.
    pub profile: ProfileIn,
}

/// A profile as the interface sends it (no id).
#[derive(Deserialize)]
pub struct ProfileIn {
    #[serde(flatten)]
    pub answers: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
pub struct AnswerArgs {
    pub key: String,
    pub answer: Answer,
}

/// Climate from a ZIP code, with dates written out for people.
#[derive(Clone, Debug, Serialize)]
pub struct ClimateOut {
    pub lookup: ClimateLookup,
    /// "Nov 17", or null where frost is not normal.
    pub first_frost: Option<String>,
    pub last_frost: Option<String>,
    /// Whether hard freezes are normal (outdoor pipes need winterising).
    pub freezes: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileOut {
    pub profile: Option<Profile>,
    pub climate: Option<ClimateOut>,
}

/// A switched-on or suggested task with its reason (P2, P9).
#[derive(Clone, Debug, Serialize)]
pub struct PlanTask {
    pub card: Card,
    pub why: Option<String>,
    pub source: Option<String>,
}

/// "Your starting plan".
#[derive(Clone, Debug, Serialize)]
pub struct StartingPlanOut {
    /// The tasks that are switched on, most important first.
    pub switched_on: Vec<PlanTask>,
    /// How many things are recorded for the home ("Your home, so far").
    pub things_recorded: usize,
    /// Things added by this answer (0 when the questions are answered again
    /// and nothing new came up).
    pub new_things: usize,
    /// Suggested tasks that are still switched off.
    pub suggested_count: usize,
    pub climate: Option<ClimateOut>,
}

/// Every suggested (switched-off) task, grouped by thing.
#[derive(Clone, Debug, Serialize)]
pub struct SuggestedGroup {
    pub item_id: Option<String>,
    pub item_name: String,
    pub category: Option<String>,
    pub tasks: Vec<PlanTask>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnswerOut {
    pub switched_on: Vec<Card>,
}
