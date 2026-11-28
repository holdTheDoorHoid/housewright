//! Typed identifiers.
//!
//! Every entity is identified by a UUIDv7: random enough to be created offline on
//! any device without coordination, and time-ordered so they sort roughly by
//! creation. Each entity kind gets its own newtype so a `TaskId` can never be
//! passed where an `ItemId` is expected.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($($(#[$meta:meta])* $name:ident),* $(,)?) => {$(
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// A fresh, time-ordered identifier (UUIDv7).
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    )*};
}

define_id!(
    /// A household: the sharing and encryption boundary.
    HouseholdId,
    /// A person in a household.
    MemberId,
    /// A house.
    PropertyId,
    /// A floor of a property.
    FloorId,
    /// A room on a floor.
    RoomId,
    /// A thing in the house.
    ItemId,
    /// A piece of maintenance work.
    TaskId,
    /// A record that a task was done.
    CompletionId,
    /// A record that a task was snoozed.
    SnoozeId,
    /// A usage counter.
    MeterId,
    /// One reading of a meter.
    MeterReadingId,
    /// A contractor.
    ContractorId,
    /// A file reference.
    AttachmentId,
    /// A free-text note attached to something.
    NoteId,
    /// A home facts sheet entry.
    FactId,
    /// An answer to a weekly suggestion.
    SuggestionAnswerId,
    /// Details (costs, values) added to a completion afterwards.
    CompletionDetailId,
    /// An operation in the operation log.
    OpId,
);
