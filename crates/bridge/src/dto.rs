//! What goes in and out of the bridge's local API. Every field here was
//! chosen on purpose: nothing marked sensitive (D-41) has a place to go.
//! `integrations/home-assistant/tests/fixtures/bridge-schema.json` lists
//! these shapes for the Python side; a test checks it is up to date.

use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use housewright_core::model::{Member, MemberKind};

#[derive(Clone, Debug, Serialize)]
pub struct StatusOut {
    pub service: String,
    pub version: String,
    /// The bridge has joined a household.
    pub paired: bool,
    pub household: Option<String>,
    /// The bridge's own member.
    pub member: Option<MemberOut>,
    pub sync: SyncOut,
    /// Plain words when something needs attention.
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncOut {
    pub enabled: bool,
    pub server: Option<String>,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
    pub pending_changes: u64,
}

impl SyncOut {
    pub fn from_status(v: &Value) -> Self {
        let s = |k: &str| v[k].as_str().map(String::from);
        Self {
            enabled: v["enabled"].as_bool().unwrap_or(false),
            server: s("server"),
            last_sync: s("last_sync"),
            last_error: s("last_error"),
            pending_changes: v["pending_changes"].as_u64().unwrap_or(0),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MemberOut {
    pub id: String,
    pub name: String,
    /// The Home Assistant bridge itself.
    pub home_assistant: bool,
}

impl MemberOut {
    pub fn of(m: &Member) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name.clone(),
            home_assistant: m.kind == MemberKind::HomeAssistant,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskOut {
    pub id: String,
    pub title: String,
    /// `ok`, `due_soon`, `due`, `overdue`, `snoozed` or `paused`.
    pub state: String,
    pub state_label: String,
    pub active: bool,
    pub due: Option<Date>,
    pub earliest: Option<Date>,
    pub overdue_after: Option<Date>,
    /// Days from today to the due date (negative once past).
    pub days_until: Option<i64>,
    pub last_done: Option<Date>,
    pub item_id: Option<String>,
    pub item_name: Option<String>,
    pub room_name: Option<String>,
    pub owner_id: String,
    pub owner_name: String,
    pub priority: String,
    pub priority_label: String,
    pub schedule_text: String,
    pub minutes: Option<u32>,
    /// Usage tasks: the meter key, and how far along.
    pub meter: Option<String>,
    pub usage_used: Option<f64>,
    pub usage_threshold: Option<f64>,
    /// Triggered tasks: the event that makes them due.
    pub event: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeterOut {
    pub key: String,
    pub name: String,
    pub unit: String,
    pub last_value: Option<f64>,
    pub last_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemOut {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub room_name: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub template: Option<String>,
    pub meters: Vec<MeterOut>,
    /// Events its tasks follow (`leak`, `battery_low`).
    pub events: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnapshotOut {
    pub status: StatusOut,
    pub tasks: Vec<TaskOut>,
    pub items: Vec<ItemOut>,
    pub members: Vec<MemberOut>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteIn {
    /// Task id or exact title.
    pub task: String,
    /// Who did it: member id or name. Empty: "Home Assistant".
    #[serde(default)]
    pub member: Option<String>,
    #[serde(default)]
    pub at: Option<Timestamp>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnoozeIn {
    pub task: String,
    #[serde(default)]
    pub until: Option<Date>,
    #[serde(default)]
    pub days: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadingIn {
    /// Item id or exact name.
    pub item: String,
    /// Meter key (`loads`) or name.
    pub meter: String,
    pub value: f64,
    /// Add `value` to the last reading instead (one more washer cycle).
    #[serde(default)]
    pub delta: bool,
    /// From an automatic sensor push: skip unchanged values and store at
    /// most one reading per meter every 30 minutes.
    #[serde(default)]
    pub throttle: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReadingOut {
    pub stored: bool,
    /// Why not: `unchanged` or `too soon`.
    pub reason: Option<String>,
    pub meter: String,
    pub value: f64,
    /// Usage tasks on that meter, after the reading.
    pub tasks: Vec<TaskOut>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventIn {
    pub item: String,
    /// `leak`, `battery_low`, ...
    pub event: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventOut {
    pub recorded: bool,
    pub event: String,
    pub message: String,
    /// Suggested tasks this event switched on.
    pub switched_on: Vec<String>,
    /// Tasks on the item that follow this event, after it.
    pub tasks: Vec<TaskOut>,
}

/// A Home Assistant device.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceIn {
    /// Home Assistant's device id (echoed back, never stored).
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Area name, which becomes the room.
    #[serde(default)]
    pub area: Option<String>,
    /// Template chosen by the person (import only).
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevicesIn {
    pub devices: Vec<DeviceIn>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceMatch {
    pub id: String,
    pub name: String,
    pub template: Option<String>,
    pub template_name: Option<String>,
    /// Already recorded as this thing.
    pub existing_item: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ImportOut {
    pub added: Vec<Imported>,
    pub skipped: Vec<Skipped>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Imported {
    pub id: String,
    pub item_id: String,
    pub name: String,
    pub template: String,
    pub suggested_tasks: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Skipped {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairIn {
    /// Optional when `code` is the whole QR text.
    #[serde(default)]
    pub server: Option<String>,
    pub code: String,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmIn {
    pub matches: bool,
}
