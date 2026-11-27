//! The operation log (D-13).
//!
//! Every change is an [`Op`]: `{id, hlc, author, entity kind, entity id, field,
//! value}`. Ops are applied to SQLite by [`crate::store::Store`]; the same ops,
//! in any order and with duplicates, produce the same state on every device.
//!
//! Merge rules:
//!
//! * **Ordinary fields** (any field name not starting with `$`): last writer
//!   wins per field. "Later" is the total order [`OrderKey`]: HLC, then author
//!   id, then op id. A JSON `null` value clears the field.
//! * **`$record`** on append-only kinds (completions, snoozes, meter readings,
//!   attachments, notes): the whole record, written once. If two different
//!   records ever claim the same id (a bug), the one with the *smallest*
//!   order key wins everywhere, so replicas still agree.
//! * **`$deleted`**: a tombstone. Deletes win over every edit, before or after
//!   (an edit cannot resurrect a deleted entity). Completions and snoozes cannot
//!   be deleted; such ops are logged but have no effect.
//! * Field ops aimed at append-only kinds, `$record` aimed at ordinary kinds and
//!   unknown `$` fields are logged and ignored, deterministically.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hlc::Hlc;
use crate::ids::{MemberId, OpId};
use crate::model::EntityKind;

/// The field name of a whole-record write on an append-only kind.
pub const RECORD_FIELD: &str = "$record";
/// The field name of a tombstone.
pub const DELETED_FIELD: &str = "$deleted";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    pub hlc: Hlc,
    pub author: MemberId,
    pub kind: EntityKind,
    pub entity: String,
    pub field: String,
    pub value: Value,
}

impl Op {
    pub fn order_key(&self) -> OrderKey {
        OrderKey::new(self.hlc, self.author, self.id)
    }
}

/// The total order used for last-writer-wins: HLC, then author (the tie-break
/// D-13 calls for), then op id (the same member on two devices can, rarely,
/// produce the same HLC). Stored as a fixed-width string whose lexical order is
/// this order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderKey(pub String);

impl OrderKey {
    pub fn new(hlc: Hlc, author: MemberId, id: OpId) -> Self {
        // Hyphenated UUIDs are fixed-width lowercase hex, so this sorts right.
        OrderKey(format!("{hlc}|{author}|{id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn order_key_tie_breaks_on_author_then_op() {
        let h = Hlc {
            wall_ms: 10,
            counter: 0,
        };
        let a1 = MemberId(Uuid::from_u128(1));
        let a2 = MemberId(Uuid::from_u128(2));
        let o1 = OpId(Uuid::from_u128(9));
        let o2 = OpId(Uuid::from_u128(3));
        assert!(OrderKey::new(h, a1, o1) < OrderKey::new(h, a2, o2));
        assert!(OrderKey::new(h, a1, o2) < OrderKey::new(h, a1, o1));
        let later = Hlc {
            wall_ms: 10,
            counter: 1,
        };
        assert!(OrderKey::new(h, a2, o1) < OrderKey::new(later, a1, o2));
    }
}
