//! Housewright core: domain model, schedule engine, prioritisation, storage
//! and the operation log. No platform code, no user interface.
//!
//! Licensed AGPL-3.0-only.

pub mod audit;
pub mod backup;
pub mod blobs;
pub mod climate;
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
pub mod priority;
pub mod profile;
pub mod records;
pub mod reminders;
pub mod repo;
pub mod schedule;
pub mod store;
pub mod suggest;
pub mod template;

pub use error::{Error, Result};
