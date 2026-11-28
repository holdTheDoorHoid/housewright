//! Housewright core: domain model, schedule engine, prioritisation, storage
//! and the operation log. No platform code, no user interface.
//!
//! Licensed AGPL-3.0-only.

pub mod audit;
pub mod error;
pub mod export;
pub mod fields;
pub mod hlc;
pub mod ids;
pub mod model;
pub mod oplog;
pub mod priority;
pub mod reminders;
pub mod repo;
pub mod schedule;
pub mod store;
pub mod template;

pub use error::{Error, Result};
