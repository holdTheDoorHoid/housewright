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
pub use housewright_plan::{Point, Units};

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
    /// Append-only record of a house binder print: which sections, and a
    /// fingerprint of each one's content, so the app can say what changed
    /// since (D-42, D-82).
    BinderPrint,
    /// A wall on a floor plan (D-25). One record per wall, so two people
    /// editing different walls merge cleanly.
    Wall,
    /// A door, window or gap in a wall.
    Opening,
    /// A pin on a floor plan: a thing, or a labelled spot (a shutoff).
    PlanPin,
    /// A free text label on a floor plan.
    PlanLabel,
    /// "Not mine": a product recall the household has looked at and says
    /// does not apply to one of its things (Phase 9). Append-only; removing
    /// it (a tombstone) brings the match back.
    RecallDismissal,
}

impl EntityKind {
    pub const ALL: [EntityKind; 26] = [
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
        EntityKind::BinderPrint,
        EntityKind::Wall,
        EntityKind::Opening,
        EntityKind::PlanPin,
        EntityKind::PlanLabel,
        EntityKind::RecallDismissal,
    ];

    /// The floor plan kinds (Phase 8, schema 6).
    pub const PLAN: [EntityKind; 4] = [
        EntityKind::Wall,
        EntityKind::Opening,
        EntityKind::PlanPin,
        EntityKind::PlanLabel,
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
            EntityKind::BinderPrint => "binder_print",
            EntityKind::Wall => "wall",
            EntityKind::Opening => "opening",
            EntityKind::PlanPin => "plan_pin",
            EntityKind::PlanLabel => "plan_label",
            EntityKind::RecallDismissal => "recall_dismissal",
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
                | EntityKind::BinderPrint
                | EntityKind::RecallDismissal
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
                | EntityKind::BinderPrint
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
    /// How lengths and areas are shown on floor plans; absent means feet
    /// and inches (D-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<Units>,
}
entity!(Household, Household);

impl Household {
    pub fn currency(&self) -> &str {
        self.currency.as_deref().unwrap_or("USD")
    }
    pub fn units(&self) -> Units {
        self.units.unwrap_or_default()
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

/// What a member is: a person, or the Home Assistant bridge (D-117), a
/// headless device that records readings and events and marks tasks done
/// on people's behalf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    #[default]
    Person,
    HomeAssistant,
}

impl MemberKind {
    pub fn is_person(&self) -> bool {
        *self == MemberKind::Person
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub name: String,
    pub role: Role,
    /// Left out of files for people, so older files read the same.
    #[serde(default, skip_serializing_if = "MemberKind::is_person")]
    pub kind: MemberKind,
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
    /// A photo or PDF page of an existing plan shown dimmed behind this
    /// floor's plan, to trace over (D-25, Stage A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<PlanBackground>,
}
entity!(Floor, Floor);

/// A traced-over background: an attachment (about the floor) placed on the
/// plan. Its top-left corner is at `at` and it is `width` millimetres wide
/// on the plan (the height follows from the picture). One field, so a move
/// and a resize made at once on two devices never mix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanBackground {
    pub attachment: AttachmentId,
    pub at: Point,
    pub width: i64,
    /// 0 to 100.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
}

fn default_opacity() -> u8 {
    35
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub property: PropertyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<FloorId>,
    pub name: String,
    /// The room's outline on its floor's plan, in millimetres. Optional:
    /// rooms without a plan work as before (P3). One field, so concurrent
    /// edits of the same room's shape resolve to one whole outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<Point>>,
}
entity!(Room, Room);

/// A wall: a straight segment between two points on a floor, with a
/// thickness. Each end is its own field, so moving one end on one device
/// and the other end on another both survive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub id: WallId,
    pub floor: FloorId,
    pub a: Point,
    pub b: Point,
    /// Millimetres; absent means [`Wall::DEFAULT_THICKNESS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<i64>,
}
entity!(Wall, Wall);

impl Wall {
    /// About 4½ inches: a finished interior stud wall.
    pub const DEFAULT_THICKNESS: i64 = 115;
    pub fn thickness(&self) -> i64 {
        self.thickness.unwrap_or(Self::DEFAULT_THICKNESS)
    }
    pub fn length(&self) -> i64 {
        housewright_plan::distance_mm(self.a, self.b)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Door,
    Window,
    /// A gap with no door (an archway).
    Gap,
}

/// Which way a door opens: hinged at the wall's `a` end or `b` end of the
/// opening, swinging to the left or right of the wall (looking from `a`
/// to `b`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swing {
    pub hinge: WallEnd,
    pub side: Side,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallEnd {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

/// A door, window or gap on a wall: `offset` millimetres from the wall's
/// `a` end to the near edge, `width` wide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    pub id: OpeningId,
    pub floor: FloorId,
    pub wall: WallId,
    pub kind: OpeningKind,
    pub offset: i64,
    pub width: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swing: Option<Swing>,
}
entity!(Opening, Opening);

/// A pin on a floor plan: a thing (`item`), or a labelled spot with no
/// thing behind it ("Water main shutoff"). A pin may sit on a wall.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanPin {
    pub id: PlanPinId,
    pub floor: FloorId,
    pub at: Point,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall: Option<WallId>,
    /// A pin can give away where valuables are (a safe): sensitive pins,
    /// and pins of sensitive things, stay out of the main binder (D-41).
    #[serde(default)]
    pub sensitivity: Sensitivity,
}
entity!(PlanPin, PlanPin);

/// Free text on a floor plan ("Stairs down", "Crawl space hatch").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanLabel {
    pub id: PlanLabelId,
    pub floor: FloorId,
    pub at: Point,
    pub text: String,
    #[serde(default)]
    pub sensitivity: Sensitivity,
}
entity!(PlanLabel, PlanLabel);

/// A warranty. `expires` is always stored, so older readers keep working;
/// when it was worked out from a length (`months`) from the install or
/// purchase date, `months` says so and the date is worked out again when
/// those dates change (see [`crate::warranty`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Warranty {
    pub expires: Date,
    /// The warranty's length, when it was given as one ("5 years").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub months: Option<u32>,
    /// The manufacturer's product registration page, if known (P9: an
    /// action to go with the reminder). An https link only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_url: Option<String>,
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
    /// What it cost when bought (Phase 9, for the insurance inventory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<Money>,
    /// What the household thinks it is worth now (their own estimate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Money>,
    /// The appliance library model family the person confirmed this is
    /// (`"rheem/performance-platinum-gas"`), or [`NO_FAMILY`] when they
    /// said none of the suggestions fit (D-149).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}
entity!(Item, Item);

/// `Item.family` when the person said none of the library's suggestions
/// fit: suggestions stop until the make or model changes.
pub const NO_FAMILY: &str = "none";

/// Whose recommendation a task follows, when its thing's model family
/// differs from the generic template (D-150, P2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Follows {
    /// The manufacturer's, from the appliance library.
    Maker,
    /// The generic template's.
    Generic,
}

/// The person's choice between the manufacturer's and the generic
/// recommendation for one task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guidance {
    /// The family whose recommendation was offered.
    pub family: String,
    pub follows: Follows,
}

/// Why a task matters, and where that advice comes from (P2, P9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Why {
    pub text: String,
    pub source: String,
    /// The page the advice comes from, when there is one (an https link to
    /// the manufacturer's page for manufacturer guidance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
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
    /// Manufacturer or generic recommendation, when the person chose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<Guidance>,
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
    /// Who did the work, when another device recorded it for them (Home
    /// Assistant, D-117). `by` stays the recording device's member (D-104);
    /// this is that device's word for who did it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_by: Option<MemberId>,
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

/// Someone who works on the house (D-28).
///
/// **Private fields** (`private_rating`, `would_hire_again`,
/// `private_notes`): the household's own opinion. They are never printed
/// (the binder drops them when it gathers its data), never sent to Home
/// Assistant (the bridge never reads contractors), never shared with any
/// server extra (D-16), and travel only inside end-to-end encrypted sync
/// operations (D-14) like every other field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contractor {
    pub id: ContractorId,
    pub name: String,
    /// The business, when the person works for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(default)]
    pub trades: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// Where they work ("Philadelphia and the Main Line").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_area: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<Credential>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insurance: Option<Credential>,
    /// Private rating 1-5, never shared (D-28). Always treated as sensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_rating: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Flagged<String>>,
    /// Listed under "Who to call" on the binder's emergency page (D-81).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub emergency: bool,
    /// Private: would the household hire them again? Licence and insurance
    /// reminders go only for contractors marked yes (P5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub would_hire_again: Option<bool>,
    /// Private notes: never printed, never shared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_notes: Option<String>,
}
entity!(Contractor, Contractor);

impl Contractor {
    /// Drop the private fields (for anything that leaves the household's
    /// own screens: the binder, exports for others).
    pub fn without_private(mut self) -> Self {
        self.private_rating = None;
        self.would_hire_again = None;
        self.private_notes = None;
        self
    }
}

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
    /// Where the file was downloaded from, for a manual saved from the
    /// manufacturer's site (D-151). An https link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
    /// Where it is on the floor plan (the water main's shutoff), so the
    /// binder's emergency page can show it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<PlanPinId>,
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
    /// Who did the work: a contractor (D-28), said after Done (P1). A later
    /// detail that names one (or clears it) replaces an earlier one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contractor: Option<ContractorId>,
    /// Set when this detail says "not a contractor after all".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contractor_cleared: bool,
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

/// A house binder print (D-82): append-only. For each section printed, a
/// fingerprint of what it said, so later the app can say which sections
/// changed since. The latest print that includes a section is the one that
/// counts for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BinderPrint {
    pub id: BinderPrintId,
    pub property: PropertyId,
    /// `emergency`, `full`, `handover` or `succession`.
    pub edition: String,
    /// `main` or `sealed`.
    pub volume: String,
    pub by: MemberId,
    pub at: Timestamp,
    /// The printed-on date shown in every page footer.
    pub printed_on: Date,
    /// Section key to fingerprint, for the sections in this print.
    pub sections: BTreeMap<String, String>,
}
entity!(BinderPrint, BinderPrint);

/// "Not mine" for one recall and one thing (Phase 9). Append-only; a
/// tombstone undoes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecallDismissal {
    pub id: RecallDismissalId,
    pub item: ItemId,
    /// The CPSC recall number ("26158").
    pub recall: String,
    pub by: MemberId,
    pub at: Timestamp,
}
entity!(RecallDismissal, RecallDismissal);
