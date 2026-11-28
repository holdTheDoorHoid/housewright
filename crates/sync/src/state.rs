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
    /// When this batch re-seals one made under an older epoch (L4): the
    /// original, in case the server already has it.
    #[serde(default)]
    pub replaces: Option<SealedBatch>,
}

/// The newest batch accepted from a device: its number and hash (D-99).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Head {
    pub seq: u64,
    pub hash: String,
}

impl Head {
    pub fn start() -> Self {
        Head {
            seq: 0,
            hash: housewright_crypto::batch::ZERO_HASH.to_string(),
        }
    }
}

/// Something about the household's devices changed (M3): shown on every
/// device until dismissed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub at_ms: u64,
    pub text: String,
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
    /// Changes to the household's devices made from other devices.
    #[serde(default)]
    pub notices: Vec<Notice>,
    /// The newest roster version another device's batch named (D-103).
    #[serde(default)]
    pub newest_roster_seen: u64,
    /// Two devices were shown different histories by the server.
    #[serde(default)]
    pub fork: bool,
    /// Batches too large to fetch, skipped (D-100).
    #[serde(default)]
    pub skipped_too_large: u64,
    /// Local changes too large to send, kept on this device only.
    #[serde(default)]
    pub unsent_too_large: u64,
    /// Devices whose changes have a gap the server cannot fill, by device
    /// name: their later changes wait here until it is filled (D-107).
    #[serde(default)]
    pub missing_from: Vec<String>,
    /// Batches whose roster claim could not be checked (a sign of a
    /// tampering device; never stops anything, D-106).
    #[serde(default)]
    pub bad_claims: u64,
}

pub(crate) struct State {
    pub config: SyncConfig,
    pub secret: DeviceSecret,
    pub chain: KeyChain,
    pub rosters: Vec<SignedRoster>,
    pub bodies: Vec<RosterBody>,
    pub cursor: u64,
    /// Local ops up to here are sealed into saved batches.
    pub push_cursor: i64,
    /// The newest of this device's batches the server acknowledged, and the
    /// local op it covers (D-107). Sealed batches are kept (`sent:<seq>`)
    /// so they can be sent again to a server restored from a backup.
    pub acked_seq: u64,
    pub acked_upto: i64,
    pub next_device_seq: u64,
    pub heads: BTreeMap<DeviceId, Head>,
    /// The hash of this device's own newest sealed batch.
    pub own_head: String,
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
            acked_seq: get(store, "acked_seq")?.unwrap_or(0),
            acked_upto: get(store, "acked_upto")?.unwrap_or(0),
            heads: get(store, "heads")?.unwrap_or_default(),
            own_head: get(store, "own_head")?
                .unwrap_or_else(|| housewright_crypto::batch::ZERO_HASH.to_string()),
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
        let acked_seq = put(&self.acked_seq);
        let acked_upto = put(&self.acked_upto);
        let heads = put(&self.heads);
        let own_head = put(&self.own_head);
        let status = put(&self.status);
        store.sync_set_many(&[
            ("config", &config),
            ("device_secret", &secret),
            ("chain", &chain),
            ("rosters", &rosters),
            ("cursor", &cursor),
            ("push_cursor", &push_cursor),
            ("next_device_seq", &next),
            ("acked_seq", &acked_seq),
            ("acked_upto", &acked_upto),
            ("heads", &heads),
            ("own_head", &own_head),
            ("status", &status),
        ])?;
        Ok(())
    }
}
