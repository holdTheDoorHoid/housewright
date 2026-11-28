//! Housewright core: domain model, schedule engine, prioritisation, storage
//! and the operation log. No platform code, no user interface.
//!
//! Licensed AGPL-3.0-only.

pub mod alerts;
pub mod audit;
pub mod backup;
pub mod blobs;
pub mod climate;
pub mod contractors;
pub mod costs;
pub mod error;
pub mod export;
pub mod facts;
pub mod fields;
pub mod hlc;
pub mod ids;
pub mod labels;
pub mod model;
pub mod onboard;
pub mod oplog;
pub mod plan;
pub mod priority;
pub mod profile;
pub mod recalls;
pub mod records;
pub mod reminders;
pub mod repo;
pub mod schedule;
pub mod store;
pub mod suggest;
pub mod template;
pub mod warranty;

pub use error::{Error, Result};

/// A date as people write it: "Nov 27, 2026".
pub fn text_date(d: jiff::civil::Date) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {}, {}",
        MONTHS[usize::from(d.month().clamp(1, 12) as u8) - 1],
        d.day(),
        d.year()
    )
}
