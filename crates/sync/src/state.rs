//! What this device remembers about sync, kept in the `sync_state` table
//! of its own database (D-95). Never exported, never synced.
//!
//! Keys at rest: the device secret and the household key chain are kept
//! there too, base64-encoded, protected only by the platform's app-private
//! storage (Android's app data folder; the browser's origin-private file
//! system). Android Keystore and WebCrypto wrapping are TODO (D-95).

use std::collections::BTreeMap;

use housewright_core::store::Store;
use housewright_crypto::batch::SealedBatch;
use housewright_crypto::encoding::{b64, unb64};
use housewright_crypto::roster::{DeviceRole, RosterBody, SignedRoster, verify_chain};
use housewright_crypto::{DeviceId, DeviceSecret, KeyChain, SpaceId};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Result, SyncError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConfig {
    pub server: String,
    pub space: SpaceId,
    pub device: DeviceId,
    pub member: String,
    pub role: DeviceRole,
}

/// A sealed batch saved before sending, and the last local op it covers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Pending {
    pub batch: SealedBatch,
    pub upto: i64,
}

/// For the interface.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_sync_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last_error_code: Option<String>,
    /// Server time minus this device's time, when it is more than a minute.
    pub clock_offset_ms: Option<i64>,
    pub clock_ahead: bool,
    pub removed: bool,
    /// Batches refused by the checks (bad signature, unknown device, ...).
    pub rejected_batches: u64,
    /// Operations refused by the clock guard.
    pub refused_future_ops: u64,
    pub recovery_confirmed: bool,
}

pub(crate) struct State {
    pub config: SyncConfig,
    pub secret: DeviceSecret,
    pub chain: KeyChain,
    pub rosters: Vec<SignedRoster>,
    pub bodies: Vec<RosterBody>,
    pub cursor: u64,
    pub push_cursor: i64,
    pub next_device_seq: u64,
    pub pending: Option<Pending>,
    pub seen: BTreeMap<DeviceId, u64>,
    pub status: SyncStatus,
}

fn get<T: for<'de> Deserialize<'de>>(store: &Store, key: &str) -> Result<Option<T>> {
    match store.sync_get(key)? {
        Some(v) => serde_json::from_str(&v)
            .map(Some)
            .map_err(|e| SyncError::Invalid(format!("sync state '{key}' is damaged: {e}"))),
        None => Ok(None),
    }
}

fn put<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).expect("sync state serialises")
}

pub(crate) fn is_set_up(store: &Store) -> Result<bool> {
    Ok(store.sync_get("config")?.is_some())
}

pub(crate) fn load_status(store: &Store) -> Result<SyncStatus> {
    Ok(get(store, "status")?.unwrap_or_default())
}

pub(crate) fn save_status(store: &Store, s: &SyncStatus) -> Result<()> {
    store.sync_set("status", &put(s))?;
    Ok(())
}

impl State {
    pub fn load(store: &Store) -> Result<State> {
        let config: SyncConfig = get(store, "config")?.ok_or(SyncError::NotSetUp)?;
        let secret_b64: Zeroizing<String> = Zeroizing::new(
            store
                .sync_get("device_secret")?
                .ok_or(SyncError::NotSetUp)?,
        );
        let secret = DeviceSecret::from_bytes(&Zeroizing::new(unb64(&secret_b64)?))?;
        let chain_b64: Zeroizing<String> =
            Zeroizing::new(store.sync_get("chain")?.ok_or(SyncError::NotSetUp)?);
        let chain = KeyChain::from_bytes(&Zeroizing::new(unb64(&chain_b64)?))?;
        let rosters: Vec<SignedRoster> = get(store, "rosters")?.ok_or(SyncError::NotSetUp)?;
        // Re-check what we stored: a damaged or edited file is refused.
        let bodies = verify_chain(&rosters)?;
        Ok(State {
            config,
            secret,
            chain,
            rosters,
            bodies,
            cursor: get(store, "cursor")?.unwrap_or(0),
            push_cursor: get(store, "push_cursor")?.unwrap_or(0),
            next_device_seq: get(store, "next_device_seq")?.unwrap_or(1),
            pending: get::<Option<Pending>>(store, "pending")?.flatten(),
            seen: get(store, "seen")?.unwrap_or_default(),
            status: load_status(store)?,
        })
    }

    pub fn latest(&self) -> &RosterBody {
        self.bodies.last().expect("a verified chain is never empty")
    }

    /// Write everything (one transaction).
    pub fn save(&self, store: &Store) -> Result<()> {
        let secret = Zeroizing::new(b64(&self.secret.to_bytes()));
        let chain = Zeroizing::new(b64(&self.chain.to_bytes()));
        let config = put(&self.config);
        let rosters = put(&self.rosters);
        let cursor = put(&self.cursor);
        let push_cursor = put(&self.push_cursor);
        let next = put(&self.next_device_seq);
        let pending = put(&self.pending);
        let seen = put(&self.seen);
        let status = put(&self.status);
        store.sync_set_many(&[
            ("config", &config),
            ("device_secret", &secret),
            ("chain", &chain),
            ("rosters", &rosters),
            ("cursor", &cursor),
            ("push_cursor", &push_cursor),
            ("next_device_seq", &next),
            ("pending", &pending),
            ("seen", &seen),
            ("status", &status),
        ])?;
        Ok(())
    }
}
