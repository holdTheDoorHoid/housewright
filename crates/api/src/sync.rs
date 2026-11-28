//! Sync commands (Phase 6). The platform gives the [`Api`] a transport
//! ([`Api::set_transport`]); without one, sync shows as unavailable and
//! the app stays local-only, which is also the default (D-11).

use housewright_core::model::Role;
use housewright_sync::{
    DeviceInfo, InviteOut, Invitee, PairState, SyncClient, SyncError, SyncReport, Transport,
};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

impl From<SyncError> for ApiError {
    fn from(e: SyncError) -> Self {
        ApiError {
            code: e.code().to_string(),
            message: e.plain(),
        }
    }
}

/// Commands under `sync_`.
pub const SYNC_COMMANDS: &[&str] = &[
    "sync_status",
    "sync_check_server",
    "sync_setup",
    "sync_confirm_recovery",
    "sync_now",
    "sync_devices",
    "sync_invite",
    "sync_invite_poll",
    "sync_invite_confirm",
    "sync_invite_cancel",
    "sync_join",
    "sync_join_poll",
    "sync_join_confirm",
    "sync_restore",
    "sync_remove_device",
    "sync_new_recovery_key",
    "sync_stop",
    "sync_dismiss_notices",
];

/// These work before a household exists on this device.
const BEFORE_INIT: &[&str] = &[
    "sync_status",
    "sync_check_server",
    "sync_join",
    "sync_join_poll",
    "sync_join_confirm",
    "sync_restore",
];

#[derive(Serialize)]
pub struct SyncStatusOut {
    /// This build can sync (it has a network transport).
    pub available: bool,
    pub enabled: bool,
    pub server: Option<String>,
    pub role: Option<String>,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
    pub last_error_code: Option<String>,
    /// Plain words when this device's clock looks wrong.
    pub clock_warning: Option<String>,
    pub clock_ahead: bool,
    pub removed: bool,
    pub pending_changes: u64,
    pub recovery_confirmed: bool,
    pub refused: u64,
    /// Changes to the household's devices made elsewhere (M3).
    pub notices: Vec<NoticeOut>,
    /// Plain words when something needs attention (a hidden or forked
    /// device list, changes too big to send, batches too big to fetch).
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct NoticeOut {
    pub at: Option<String>,
    pub text: String,
}

#[derive(Deserialize)]
struct ServerArgs {
    server: String,
}

#[derive(Deserialize)]
struct SetupArgs {
    server: String,
    device_name: String,
    #[serde(default)]
    registration_token: Option<String>,
}

#[derive(Serialize)]
struct KeyOut {
    /// Shown once. Not stored anywhere on the device.
    recovery_key: String,
    status: SyncStatusOut,
}

#[derive(Deserialize)]
struct InviteArgs {
    /// `device` (another device of mine) or `person`.
    who: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    role: Option<Role>,
}

#[derive(Serialize)]
struct InviteResult {
    #[serde(flatten)]
    invite: InviteOut,
    qr_svg: String,
    expires: String,
}

#[derive(Deserialize)]
struct ConfirmArgs {
    matches: bool,
}

#[derive(Deserialize)]
struct JoinArgs {
    /// The server address and code typed separately, or `code` may hold
    /// the whole QR text (then `server` may be empty).
    #[serde(default)]
    server: String,
    code: String,
    device_name: String,
}

#[derive(Deserialize)]
struct RemoveArgs {
    id: String,
    #[serde(default)]
    recovery_key: Option<String>,
}

#[derive(Deserialize)]
struct KeyArgs {
    #[serde(default)]
    recovery_key: String,
}

#[derive(Deserialize)]
struct RestoreArgs {
    server: String,
    recovery_key: String,
    device_name: String,
}

#[derive(Serialize)]
struct SyncNowOut {
    report: SyncReport,
    status: SyncStatusOut,
}

#[derive(Serialize)]
struct DeviceOut {
    #[serde(flatten)]
    d: DeviceInfo,
    added: Option<String>,
    last_seen: Option<String>,
}

fn unavailable() -> ApiError {
    ApiError {
        code: "unavailable".into(),
        message: "This version of the app cannot sync.".into(),
    }
}

fn ts(ms: Option<u64>) -> Option<String> {
    ms.and_then(|m| Timestamp::from_millisecond(m as i64).ok())
        .map(|t| t.to_string())
}

impl Api {
    /// Give the API a way to reach a sync server (native HTTP on Android
    /// and desktop; synchronous XHR in the web worker).
    pub fn set_transport(&mut self, t: Box<dyn Transport>) {
        self.sync = Some(SyncClient::new(t));
    }

    pub(crate) fn sync_status(&self) -> Result<SyncStatusOut, ApiError> {
        let cfg = SyncClient::config(&self.store)?;
        let st = match &self.sync {
            Some(c) => c.status(&self.store)?,
            None => Default::default(),
        };
        let clock_warning = st.clock_offset_ms.filter(|o| o.unsigned_abs() > 5 * 60_000).map(|o| {
            format!(
                "This device's clock looks wrong: it is {} minutes {} the sync server's. Set the date and time automatically in this device's settings.",
                o.unsigned_abs() / 60_000,
                if o > 0 { "behind" } else { "ahead of" }
            )
        });
        Ok(SyncStatusOut {
            available: self.sync.is_some(),
            enabled: cfg.is_some(),
            server: cfg.as_ref().map(|c| c.server.clone()),
            role: cfg.as_ref().map(|c| c.role.as_str().to_string()),
            last_sync: ts(st.last_sync_ms),
            last_error: st.last_error,
            last_error_code: st.last_error_code,
            clock_warning,
            clock_ahead: st.clock_ahead,
            removed: st.removed,
            pending_changes: if cfg.is_some() {
                SyncClient::pending_count(&self.store)?
            } else {
                0
            },
            recovery_confirmed: st.recovery_confirmed,
            refused: st.rejected_batches + st.refused_future_ops,
            notices: st
                .notices
                .iter()
                .map(|n| NoticeOut {
                    at: ts(Some(n.at_ms)),
                    text: n.text.clone(),
                })
                .collect(),
            warnings: {
                let mut w = Vec::new();
                if st.fork {
                    w.push("The sync server showed this device a different history of the household's devices than another device saw. Changes made here are kept on this device until you press OK below. Check who runs the server.".to_string());
                }
                if st.unsent_too_large > 0 {
                    w.push(format!(
                        "{} change(s) were too large to send and stay on this device only.",
                        st.unsent_too_large
                    ));
                }
                if !st.missing_from.is_empty() {
                    w.push(format!(
                        "Some changes from {} are missing on the sync server. Later changes from that device wait on this one until they arrive; that device sends them again when it next syncs.",
                        st.missing_from.join(", ")
                    ));
                }
                if st.bad_claims > 0 {
                    w.push(format!(
                        "{} change batch(es) from another device named a household device list that could not be checked. They were applied as usual; if this keeps happening, a device may be misbehaving.",
                        st.bad_claims
                    ));
                }
                if st.skipped_too_large > 0 {
                    w.push(format!(
                        "{} batch(es) on the server were too large to fetch and were skipped.",
                        st.skipped_too_large
                    ));
                }
                w
            },
        })
    }

    /// Sync now if sync is on; errors are recorded in the status, not
    /// returned (the app works the same either way).
    pub fn sync_quietly(&mut self) {
        if let Some(c) = self.sync.as_mut()
            && SyncClient::is_set_up(&self.store).unwrap_or(false)
        {
            let _ = c.sync(&mut self.store, self.blobs.as_mut());
        }
    }

    /// A file not on this device yet: ask the sync server for it.
    pub(crate) fn fetch_missing_file(&mut self, hash: &str) -> Option<Vec<u8>> {
        let c = self.sync.as_mut()?;
        c.fetch_file(&self.store, self.blobs.as_mut(), hash)
            .ok()
            .flatten()
    }

    pub(crate) fn sync_call(&mut self, cmd: &str, a: Value) -> Result<Value, ApiError> {
        if !BEFORE_INIT.contains(&cmd) && self.store.meta("household_id")?.is_none() {
            return Err(ApiError {
                code: "not_initialized".into(),
                message: "set up a household first".into(),
            });
        }
        match cmd {
            "sync_status" => ok(self.sync_status()?),
            "sync_check_server" => {
                let a: ServerArgs = args(a)?;
                let store = &self.store;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                ok(c.health(store, &a.server)?)
            }
            "sync_setup" => {
                let a: SetupArgs = args(a)?;
                let me = self.me()?;
                if me.role != Role::Owner {
                    return Err(ApiError::invalid(
                        "Only the household's owner can set up sync.",
                    ));
                }
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let key = c.setup(
                    &mut self.store,
                    &a.server,
                    a.device_name.trim(),
                    a.registration_token.as_deref(),
                )?;
                self.sync_quietly();
                ok(KeyOut {
                    recovery_key: key.to_string(),
                    status: self.sync_status()?,
                })
            }
            "sync_confirm_recovery" => {
                SyncClient::confirm_recovery_saved(&self.store)?;
                ok(self.sync_status()?)
            }
            "sync_now" => {
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let report = c.sync(&mut self.store, self.blobs.as_mut());
                let status = self.sync_status()?;
                match report {
                    Ok(report) => ok(SyncNowOut { report, status }),
                    // Offline and similar: say so, keep the status.
                    Err(e) => Err(e.into()),
                }
            }
            "sync_devices" => {
                let store = &self.store;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let list = c.devices(store)?;
                ok(list
                    .into_iter()
                    .map(|d| DeviceOut {
                        added: ts(d.added_ms),
                        last_seen: ts(d.last_seen_ms),
                        d,
                    })
                    .collect::<Vec<_>>())
            }
            "sync_invite" => {
                let a: InviteArgs = args(a)?;
                let who = match a.who.as_str() {
                    "device" => Invitee::MyDevice,
                    "person" => Invitee::NewPerson {
                        name: a.name.unwrap_or_default(),
                        role: a.role.unwrap_or(Role::Member),
                    },
                    w => return Err(ApiError::invalid(format!("unknown invitee '{w}'"))),
                };
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let out = c.invite_start(&mut self.store, who)?;
                let qr_svg = housewright_media::qr::svg(&out.qr_text)
                    .map_err(|e| ApiError::internal(e.to_string()))?;
                ok(InviteResult {
                    expires: ts(Some(out.expires_ms)).unwrap_or_default(),
                    invite: out,
                    qr_svg,
                })
            }
            "sync_invite_poll" => {
                let store = &self.store;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                ok(c.invite_poll(store)?)
            }
            "sync_invite_confirm" => {
                let a: ConfirmArgs = args(a)?;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                ok(c.invite_confirm(&mut self.store, self.blobs.as_mut(), a.matches)?)
            }
            "sync_invite_cancel" => {
                let store = &self.store;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                c.invite_cancel(store)?;
                ok(())
            }
            "sync_join" => {
                let a: JoinArgs = args(a)?;
                let (server, code) = match housewright_sync::parse_qr(&a.code) {
                    Some((s, c)) => (s, c),
                    None => (a.server, a.code),
                };
                let store = &self.store;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                ok(c.join_start(store, &server, &code, a.device_name.trim())?)
            }
            "sync_join_poll" => {
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let s = c.join_poll(&mut self.store, self.blobs.as_mut())?;
                self.after_join(&s)?;
                ok(s)
            }
            "sync_join_confirm" => {
                let a: ConfirmArgs = args(a)?;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let s = c.join_confirm(&mut self.store, self.blobs.as_mut(), a.matches)?;
                self.after_join(&s)?;
                ok(s)
            }
            "sync_restore" => {
                let a: RestoreArgs = args(a)?;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                c.restore(
                    &mut self.store,
                    self.blobs.as_mut(),
                    &a.server,
                    &a.recovery_key,
                    a.device_name.trim(),
                )?;
                self.store
                    .set_meta("last_seen", &self.store.now().to_string())?;
                ok(self.status()?)
            }
            "sync_remove_device" => {
                let a: RemoveArgs = args(a)?;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                c.remove_device(
                    &mut self.store,
                    self.blobs.as_mut(),
                    &a.id,
                    a.recovery_key.as_deref(),
                )?;
                self.sync_call("sync_devices", Value::Null)
            }
            "sync_new_recovery_key" => {
                let a: KeyArgs = args(a)?;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                let key = c.new_recovery_key(&mut self.store, &a.recovery_key)?;
                ok(KeyOut {
                    recovery_key: key.to_string(),
                    status: self.sync_status()?,
                })
            }
            "sync_dismiss_notices" => {
                SyncClient::dismiss_notices(&self.store)?;
                ok(self.sync_status()?)
            }
            "sync_stop" => {
                let store = &self.store;
                let c = self.sync.as_mut().ok_or_else(unavailable)?;
                c.forget(store)?;
                ok(self.sync_status()?)
            }
            other => Err(ApiError::invalid(format!("unknown command '{other}'"))),
        }
    }

    fn after_join(&mut self, s: &PairState) -> Result<(), ApiError> {
        if *s == PairState::Done && self.store.meta("last_seen")?.is_none() {
            self.store
                .set_meta("last_seen", &self.store.now().to_string())?;
        }
        Ok(())
    }
}
