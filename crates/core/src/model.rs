//! The domain model (ARCHITECTURE.md, "Data model").
//!
//! Every struct here is stored as a JSON object in SQLite and changed only
//! through operations (see [`crate::oplog`]). Ordinary entities merge field by
//! field ("last writer wins"); the append-only records ([`Completion`],
//! [`Snooze`], [`MeterReading`], [`Attachment`], [`Note`]) are written once and
//! never edited.
//!
//! Sensitivity (D-41): anything that could be a code, an account detail or the
//! whereabouts of valuables carries a [`Sensitivity`] flag, either directly or
//! through a [`Flagged`] wrapper, so the binder can file it into the sealed volume.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::fields::{CustomValues, FieldDef};
use crate::ids::*;
use crate::priority::PriorityClass;
use crate::reminders::ReminderPrefs;
use crate::schedule::{Climate, Schedule, WindowPolicy};

/// Normal (main binder) or sensitive (sealed volume), per D-41.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    #[default]
    Normal,
    Sensitive,
}

/// A value with its own sensitivity flag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flagged<T> {
    pub value: T,
    #[serde(default)]
    pub sensitivity: Sensitivity,
}

impl<T> Flagged<T> {
    pub fn normal(value: T) -> Self {
        Self {
            value,
            sensitivity: Sensitivity::Normal,
        }
    }
    pub fn sensitive(value: T) -> Self {
        Self {
            value,
            sensitivity: Sensitivity::Sensitive,
        }
    }
}

/// The kinds of entity the operation log knows about. The string form is also
/// the SQLite table name and the key in the export format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Household,
    Member,
    Property,
    Floor,
    Room,
    Item,
    Template,
    Task,
    Completion,
    Snooze,
    Meter,
    MeterReading,
    Contractor,
    Attachment,
    Note,
    Fact,
    /// The answers to the home questions (one per property).
    Profile,
    /// Append-only answer to a weekly suggestion (Add / Not for us / Later).
    SuggestionAnswer,
    /// Append-only details added after a completion: costs and completion
    /// field values (P1: offered after Done, never required).
    CompletionDetail,
    /// The household's own fields added to a template (D-22), kept apart
    /// from the bundled template copy so a library update cannot drop them.
    TemplateFields,
}

impl EntityKind {
    pub const ALL: [EntityKind; 20] = [
        EntityKind::Household,
        EntityKind::Member,
        EntityKind::Property,
        EntityKind::Floor,
        EntityKind::Room,
        EntityKind::Item,
        EntityKind::Template,
        EntityKind::Task,
        EntityKind::Completion,
        EntityKind::Snooze,
        EntityKind::Meter,
        EntityKind::MeterReading,
        EntityKind::Contractor,
        EntityKind::Attachment,
        EntityKind::Note,
        EntityKind::Fact,
        EntityKind::Profile,
        EntityKind::SuggestionAnswer,
        EntityKind::CompletionDetail,
        EntityKind::TemplateFields,
    ];

    /// The kinds that existed at schema version 1. Migration 1 creates
    /// exactly these tables; later kinds get theirs from later migrations.
    pub const SCHEMA_V1: [EntityKind; 16] = [
        EntityKind::Household,
        EntityKind::Member,
        EntityKind::Property,
        EntityKind::Floor,
        EntityKind::Room,
        EntityKind::Item,
        EntityKind::Template,
        EntityKind::Task,
        EntityKind::Completion,
        EntityKind::Snooze,
        EntityKind::Meter,
        EntityKind::MeterReading,
        EntityKind::Contractor,
        EntityKind::Attachment,
        EntityKind::Note,
        EntityKind::Fact,
    ];

    /// Table name / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Household => "household",
            EntityKind::Member => "member",
            EntityKind::Property => "property",
            EntityKind::Floor => "floor",
            EntityKind::Room => "room",
            EntityKind::Item => "item",
            EntityKind::Template => "template",
            EntityKind::Task => "task",
            EntityKind::Completion => "completion",
            EntityKind::Snooze => "snooze",
            EntityKind::Meter => "meter",
            EntityKind::MeterReading => "meter_reading",
            EntityKind::Contractor => "contractor",
            EntityKind::Attachment => "attachment",
            EntityKind::Note => "note",
            EntityKind::Fact => "fact",
            EntityKind::Profile => "profile",
            EntityKind::SuggestionAnswer => "suggestion_answer",
            EntityKind::CompletionDetail => "completion_detail",
            EntityKind::TemplateFields => "template_fields",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Append-only kinds are written once as a whole record and never edited.
    pub fn append_only(self) -> bool {
        matches!(
            self,
            EntityKind::Completion
                | EntityKind::Snooze
                | EntityKind::MeterReading
                | EntityKind::Attachment
                | EntityKind::Note
                | EntityKind::SuggestionAnswer
                | EntityKind::CompletionDetail
        )
    }

    /// Whether a tombstone may hide this kind. Completions are never deleted;
    /// a mistaken one is voided by a later completion record instead.
    pub fn deletable(self) -> bool {
        !matches!(
            self,
            EntityKind::Completion
                | EntityKind::Snooze
                | EntityKind::SuggestionAnswer
                | EntityKind::CompletionDetail
        )
    }
}

/// Implemented by every stored struct.
pub trait Entity: Serialize + DeserializeOwned {
    const KIND: EntityKind;
    fn entity_id(&self) -> String;
}

/// Implements [`Entity`] for a struct with an `id` field.
#[macro_export]
macro_rules! entity {
    ($t:ty, $kind:ident) => {
        impl $crate::model::Entity for $t {
            const KIND: $crate::model::EntityKind = $crate::model::EntityKind::$kind;
            fn entity_id(&self) -> String {
                self.id.to_string()
            }
        }
    };
}

/// A reference to any entity, used by notes and attachments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
}

/// The household: sharing and encryption boundary (one per database).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Household {
    pub id: HouseholdId,
    pub name: String,
    /// Default IANA time zone for new properties.
    pub time_zone: String,
    /// ISO 4217 currency for costs; absent means USD (D-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}
entity!(Household, Household);

impl Household {
    pub fn currency(&self) -> &str {
        self.currency.as_deref().unwrap_or("USD")
    }
}

/// Roles (D-23). Contractor/tenant links are not members; they arrive with
/// share links in Phase 11.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Owner/admin: everything, including members and the sealed volume.
    Owner,
    /// Household member: sees and edits the whole plan.
    Member,
    /// Limited: sees only tasks assigned to them.
    Limited,
}

impl Role {
    pub fn can_edit_plan(self) -> bool {
        matches!(self, Role::Owner | Role::Member)
    }
    pub fn can_manage_members(self) -> bool {
        matches!(self, Role::Owner)
    }
    pub fn can_see_sensitive(self) -> bool {
        matches!(self, Role::Owner)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub name: String,
    pub role: Role,
    /// Phone or email; flagged because it is personal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<Flagged<String>>,
    /// Quiet hours and preferred maintenance window (P5, D-59).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminders: Option<ReminderPrefs>,
}
entity!(Member, Member);

/// A house.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub id: PropertyId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<Flagged<String>>,
    /// IANA time zone; all due dates for this property are civil dates here.
    pub time_zone: String,
    pub climate: Climate,
}
entity!(Property, Property);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floor {
    pub id: FloorId,
    pub property: PropertyId,
    pub name: String,
    /// 0 = ground, negative = basement.
    #[serde(default)]
    pub level: i32,
}
entity!(Floor, Floor);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub property: PropertyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<FloorId>,
    pub name: String,
}
entity!(Room, Room);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Warranty {
    pub expires: Date,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default)]
    pub transferable: bool,
    /// Policy or claim numbers can be account details: flagged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Flagged<String>>,
}

/// Something in the house.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub property: PropertyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomId>,
    /// A filter inside a furnace has the furnace as its parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ItemId>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<Flagged<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchased: Option<Date>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<Date>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warranty: Option<Warranty>,
    /// Where exactly it is ("behind the panel in the hall closet"). A safe's
    /// location is the classic sensitive value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_note: Option<Flagged<String>>,
    /// Template id this item came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Extra fields added to this single item (D-22), on top of the template's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_fields: Vec<FieldDef>,
    #[serde(default)]
    pub custom: CustomValues,
    /// Whole-item sensitivity (e.g. a safe, jewellery).
    #[serde(default)]
    pub sensitivity: Sensitivity,
}
entity!(Item, Item);

/// Why a task matters, and where that advice comes from (P2, P9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Why {
    pub text: String,
    pub source: String,
}

/// Recurring (or one-off) work on an item or a place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub property: PropertyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomId>,
    pub title: String,
    /// Exactly one named owner (P8).
    pub owner: MemberId,
    pub priority: PriorityClass,
    pub schedule: Schedule,
    /// Overrides the default due window for this schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPolicy>,
    /// The date the task begins. A floating task never done is first due here;
    /// a fixed task with no anchor counts from here.
    pub start: Date,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Why>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<u32>,
    /// Fields offered after completion (never required, P1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completion_fields: Vec<FieldDef>,
    /// Template id and task key this came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<TemplateTaskRef>,
    /// Switched on in the plan (P3: start small). Inactive tasks are suggestions.
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub not_applicable: bool,
}
entity!(Task, Task);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateTaskRef {
    pub template: String,
    pub task: String,
}

/// Money in minor units (cents), to avoid floating point.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub minor: i64,
    /// ISO 4217, e.g. "USD".
    pub currency: String,
}

/// Append-only record of a task done. Never edited or deleted: a mistake is
/// corrected by a later completion with `voids` set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub id: CompletionId,
    pub task: TaskId,
    pub by: MemberId,
    pub at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Flagged<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Money>,
    #[serde(default)]
    pub values: CustomValues,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contractor: Option<ContractorId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentId>,
    /// If set, this record cancels an earlier completion (logged in error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voids: Option<CompletionId>,
}
entity!(Completion, Completion);

/// Append-only record of a snooze ("reschedule to a moment", P7).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snooze {
    pub id: SnoozeId,
    pub task: TaskId,
    pub by: MemberId,
    pub at: Timestamp,
    pub until: Date,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
entity!(Snooze, Snooze);

/// A usage counter: runtime hours, loads, gallons.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    pub id: MeterId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemId>,
    /// Machine key, matched by usage schedules (`runtime_hours`).
    pub key: String,
    pub name: String,
    pub unit: String,
}
entity!(Meter, Meter);

/// Append-only meter reading (by hand, or from Home Assistant later).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeterReading {
    pub id: MeterReadingId,
    pub meter: MeterId,
    pub at: Timestamp,
    pub value: f64,
    pub by: MemberId,
}
entity!(MeterReading, MeterReading);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub number: Flagged<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<Date>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contractor {
    pub id: ContractorId,
    pub name: String,
    #[serde(default)]
    pub trades: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<Credential>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insurance: Option<Credential>,
    /// Private rating 1-5, never shared (D-28). Always treated as sensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_rating: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Flagged<String>>,
}
entity!(Contractor, Contractor);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Manual,
    Receipt,
    Warranty,
    Photo,
    Other,
}

/// A file, by content hash only. Phase 1 stores no file bytes (D-17: files
/// will live encrypted beside the database).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: AttachmentId,
    /// Lowercase hex SHA-256 of the plaintext file.
    pub sha256: String,
    pub name: String,
    pub media_type: String,
    pub size: u64,
    pub kind: AttachmentKind,
    pub about: EntityRef,
    #[serde(default)]
    pub sensitivity: Sensitivity,
    /// A small preview (photos only), also a content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_sha256: Option<String>,
    /// Pixel size of a stored photo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Who added it, and when (absent in Phase 1-3 records).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<MemberId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Timestamp>,
}
entity!(Attachment, Attachment);

impl Attachment {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::blobs::is_hash(&self.sha256)
            || self
                .thumb_sha256
                .as_deref()
                .is_some_and(|t| !crate::blobs::is_hash(t))
        {
            return Err("sha256 must be 64 lowercase hex characters".into());
        }
        Ok(())
    }
}

/// Append-only free-text note on anything.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub about: EntityRef,
    pub by: MemberId,
    pub at: Timestamp,
    pub text: Flagged<String>,
}
entity!(Note, Note);

/// A home facts sheet entry (D-27): filter sizes, paint colours, shutoffs,
/// breakers, codes. Codes are sensitive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub id: FactId,
    pub property: PropertyId,
    pub category: String,
    pub label: String,
    pub value: String,
    pub sensitivity: Sensitivity,
    /// Set when the fact answers one of the suggested questions
    /// (`water_main`), so the question is not asked again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The room it is about (paint colours per room).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomId>,
}
entity!(Fact, Fact);

/// What a cost was for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostKind {
    Parts,
    Labour,
    Contractor,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostLine {
    pub kind: CostKind,
    pub amount: Money,
}

/// Details added to a completion after Done (P1): costs and the task's
/// completion fields. Append-only. A later detail for the same completion
/// replaces an earlier one's costs (and the values it sets), so a typo is
/// corrected by adding again, never by editing history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompletionDetail {
    pub id: CompletionDetailId,
    pub completion: CompletionId,
    pub by: MemberId,
    pub at: Timestamp,
    /// `None` leaves earlier costs as they were; `Some(vec![])` clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub costs: Option<Vec<CostLine>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: CustomValues,
}
entity!(CompletionDetail, CompletionDetail);

/// The household's own fields on a template (D-22). The id is the template id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateFields {
    pub id: String,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
}

impl Entity for TemplateFields {
    const KIND: EntityKind = EntityKind::TemplateFields;
    fn entity_id(&self) -> String {
        self.id.clone()
    }
}
