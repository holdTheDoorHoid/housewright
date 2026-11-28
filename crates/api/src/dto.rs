//! Argument and result types of the command surface. Their JSON shapes are
//! mirrored by hand in `app/src/lib/api/types.ts`; keep the two in step.

use std::collections::BTreeMap;

use housewright_core::Error;
use housewright_core::climate::ClimateLookup;
use housewright_core::fields::FieldDef;
use housewright_core::model::{AttachmentKind, CostKind, Role};
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
    /// From label reading (D-29), as the person confirmed it.
    #[serde(default)]
    pub serial: Option<String>,
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
    /// ISO 4217 code for costs (D-76).
    pub currency: Option<String>,
    /// How floor plan lengths and areas are shown (D-06).
    pub units: Option<housewright_core::model::Units>,
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
    /// Possible product recalls not yet looked at (Phase 9; 0 when recall
    /// checks are off on this device).
    pub recall_matches: usize,
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
    /// Photos and files added after Done.
    pub attachments: Vec<AttachmentOut>,
    /// The costs that count (the latest given), in minor units.
    pub costs: Vec<CostLineOut>,
    pub cost_total: Option<i64>,
    /// Completion field values (sensitive ones hidden).
    pub values: Vec<FieldOut>,
    /// Who did it, when a contractor did (D-28).
    pub contractor_id: Option<String>,
    pub contractor_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CostLineOut {
    pub kind: CostKind,
    pub minor: i64,
    pub currency: String,
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
    /// Offered after Done, never required (P1).
    pub completion_fields: Vec<FieldDef>,
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
    /// Sticker code (D-30), e.g. `HW-7K3QM`.
    pub code: String,
    /// Its most pressing switched-on task, for the sticker's one-tap Done.
    pub next: Option<Card>,
    pub attachments: Vec<AttachmentOut>,
    /// Every field it can have, with the current value for editing.
    pub edit: Vec<FieldEdit>,
    pub serial_sensitive: bool,
    /// Where it is on a floor plan, if it has a pin.
    pub pin: Option<ItemPinOut>,
    /// What it cost and what it is worth now (Phase 9).
    pub price: Option<housewright_core::model::Money>,
    pub value: Option<housewright_core::model::Money>,
    pub warranty: Option<crate::alerts::WarrantyOut>,
}

/// A thing's place on a floor plan, for the thumbnail on its page.
#[derive(Clone, Debug, Serialize)]
pub struct ItemPinOut {
    pub pin_id: String,
    pub floor_id: String,
    pub floor_name: String,
    pub at: housewright_core::model::Point,
}

/// A field on the edit form. Sensitive values are not sent; the form shows
/// "set, hidden" and a reveal button.
#[derive(Clone, Debug, Serialize)]
pub struct FieldEdit {
    pub def: FieldDef,
    /// `template`, `household` (added to the template here) or `item`.
    pub origin: &'static str,
    pub value: Option<String>,
    pub has_value: bool,
    pub sensitive: bool,
    pub photo: Option<AttachmentOut>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AttachmentOut {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub size: u64,
    pub kind: AttachmentKind,
    pub has_thumb: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub at: Option<Timestamp>,
    pub sensitive: bool,
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
    /// The task, or for a notice its stable key.
    pub task_id: String,
    pub kind: ReminderKind,
    pub at_ms: i64,
    pub title: String,
    pub body: String,
    /// For a notice: what tapping it opens (`item/<id>`, `contractor/<id>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
}

impl From<PlannedReminder> for ReminderOut {
    fn from(r: PlannedReminder) -> Self {
        Self {
            task_id: r.task.to_string(),
            kind: r.kind,
            at_ms: r.at.as_millisecond(),
            title: r.title,
            body: r.body,
            open: None,
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

// -- Phase 4: records -------------------------------------------------------

#[derive(Deserialize)]
pub struct UpdateItemArgs {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    /// For these, absent = unchanged, "" = cleared.
    #[serde(default)]
    pub make: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial: Option<String>,
    #[serde(default)]
    pub installed: Option<String>,
    #[serde(default)]
    pub purchased: Option<String>,
    #[serde(default)]
    pub room_id: Option<String>,
    /// Field values as typed; `null` or "" clears one.
    #[serde(default)]
    pub values: BTreeMap<String, Option<String>>,
    /// Amounts as typed ("1,299.99"); absent = unchanged, "" = cleared.
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    /// The warranty; absent = unchanged.
    #[serde(default)]
    pub warranty: Option<crate::alerts::WarrantyIn>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateItemOut {
    pub detail: ItemDetail,
    /// Tasks that now apply (added switched off, to offer; never switched on).
    pub offered: Vec<Card>,
    /// Switched-on tasks whose condition no longer holds.
    pub no_longer_apply: Vec<Card>,
}

#[derive(Deserialize)]
pub struct AddFieldArgs {
    pub item_id: String,
    /// Add to every thing made from this thing's template.
    #[serde(default)]
    pub to_template: bool,
    pub label: String,
    /// `text`, `number`, `date`, `yes_no`, `choice` or `photo`.
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    /// Absent: decided from the label (codes, passwords are sensitive).
    #[serde(default)]
    pub sensitive: Option<bool>,
}

#[derive(Deserialize)]
pub struct ItemKeyArgs {
    pub item_id: String,
    pub key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RevealOut {
    pub value: String,
}

#[derive(Deserialize, Default)]
pub struct CostsIn {
    #[serde(default)]
    pub parts: Option<String>,
    #[serde(default)]
    pub labour: Option<String>,
    #[serde(default)]
    pub contractor: Option<String>,
    #[serde(default)]
    pub other: Option<String>,
}

#[derive(Deserialize)]
pub struct AddDetailsArgs {
    pub completion_id: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub costs: Option<CostsIn>,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// Who did the work: a contractor's id, or "" for "not a contractor".
    /// Absent leaves it as it was.
    #[serde(default)]
    pub contractor_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FactOut {
    pub id: String,
    pub group: String,
    pub label: String,
    /// Absent when sensitive: ask `reveal_fact`.
    pub value: Option<String>,
    pub sensitive: bool,
    pub key: Option<String>,
    pub room_id: Option<String>,
    pub room_name: Option<String>,
    pub attachments: Vec<AttachmentOut>,
    /// The floor plan pin it is linked to.
    pub pin_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FactGroupOut {
    pub key: String,
    pub label: String,
    pub sensitive_by_default: bool,
    pub facts: Vec<FactOut>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FactsOut {
    pub property_name: String,
    pub groups: Vec<FactGroupOut>,
    pub prompts: Vec<housewright_core::facts::FactPrompt>,
}

#[derive(Deserialize)]
pub struct SaveFactArgs {
    #[serde(default)]
    pub id: Option<String>,
    pub group: String,
    pub label: String,
    /// Absent on an edit of a sensitive fact = keep the hidden value.
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub sensitive: Option<bool>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub room_id: Option<String>,
    /// Where it is on a floor plan: absent keeps the link, "" removes it.
    #[serde(default)]
    pub pin_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CurrencyArgs {
    pub currency: String,
}

#[derive(Deserialize)]
pub struct ItemIdsArgs {
    #[serde(default)]
    pub item_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StickerOut {
    pub item_id: String,
    pub name: String,
    pub room_name: Option<String>,
    pub code: String,
    pub url: String,
    pub qr_svg: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SheetPreset {
    pub key: String,
    pub label: String,
    pub layout: housewright_media::sheet::Layout,
}

#[derive(Clone, Debug, Serialize)]
pub struct LabelSheetOut {
    pub presets: Vec<SheetPreset>,
    pub stickers: Vec<StickerOut>,
}

#[derive(Deserialize)]
pub struct SheetPdfArgs {
    #[serde(default)]
    pub item_ids: Option<Vec<String>>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub layout: Option<housewright_media::sheet::Layout>,
}

#[derive(Deserialize)]
pub struct TextArgs {
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanOut {
    /// What the QR codes in the photo said.
    pub texts: Vec<String>,
    /// The things they point at (usually one).
    pub items: Vec<ItemRow>,
}

#[derive(Deserialize)]
pub struct AttachArgs {
    /// `item`, `completion`, `fact` or `contractor`.
    pub about_kind: String,
    pub about_id: String,
    #[serde(default)]
    pub kind: Option<AttachmentKind>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
}

#[derive(Deserialize)]
pub struct FileArgs {
    pub id: String,
    #[serde(default)]
    pub thumb: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LabelReadOut {
    /// Every line read, so the person can see what the phone saw.
    pub lines: Vec<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub model_alternatives: Vec<String>,
    pub serial_alternatives: Vec<String>,
    pub template: Option<TemplateRef>,
}
