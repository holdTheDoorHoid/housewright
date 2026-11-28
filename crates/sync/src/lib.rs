//! Housewright sync client (D-13, D-89 to D-97).
//!
//! Pushes this device's operations, pulls everyone else's, checks and
//! decrypts them, merges them with [`Store::apply_synced_ops`], and moves
//! the encrypted files attachments refer to. Every step is resumable and
//! idempotent: a batch is sealed and saved *before* it is sent, so a retry
//! after a crash or a lost connection sends exactly the same bytes, and
//! the server keeps one copy. The app keeps working offline; the next
//! sync catches up.
//!
//! The network is behind [`Transport`] (blocking request in, response
//! out). Native builds use [`native::UreqTransport`]; the web build passes
//! a synchronous XHR from its worker. Nothing here spawns threads or
//! timers, so the platform decides when to sync.
//!
//! Licensed AGPL-3.0-only.

mod client;
#[cfg(feature = "native-http")]
pub mod native;
mod pairing;
mod state;

pub use client::*;
pub use pairing::*;
pub use state::{SyncConfig, SyncStatus};

use housewright_crypto::CryptoError;
use housewright_crypto::wire::ErrorBody;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// One blocking HTTP exchange. `Err` means no answer arrived at all
/// (offline, DNS, TLS, timeout); an HTTP error status is an `Ok`.
pub trait Transport: Send {
    fn send(&mut self, req: HttpRequest) -> std::result::Result<HttpResponse, String>;
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("can't reach the sync server ({0})")]
    Offline(String),
    #[error("the server said: {message}")]
    Server {
        status: u16,
        code: String,
        message: String,
    },
    #[error("this device's clock is off by {} minutes", offset_ms.unsigned_abs() / 60_000)]
    ClockSkew { offset_ms: i64 },
    #[error("changes were made while this device's clock was ahead")]
    ClockAhead,
    #[error("this device was removed from the household")]
    Removed,
    #[error("another device knows a newer list of the household's devices")]
    RosterHidden,
    #[error("sync is not set up on this device")]
    NotSetUp,
    #[error("{0}")]
    Invalid(String),
    #[error("a security check failed: {0}")]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Core(#[from] housewright_core::Error),
}

impl SyncError {
    /// A short code for the interface.
    pub fn code(&self) -> &str {
        match self {
            SyncError::Offline(_) => "offline",
            SyncError::Server { code, .. } => code,
            SyncError::ClockSkew { .. } => "clock_skew",
            SyncError::ClockAhead => "clock_ahead",
            SyncError::Removed => "removed",
            SyncError::RosterHidden => "roster_hidden",
            SyncError::NotSetUp => "not_set_up",
            SyncError::Invalid(_) => "invalid",
            SyncError::Crypto(_) => "security",
            SyncError::Core(_) => "internal",
        }
    }

    /// What to tell a person: what happened and what to do, no jargon.
    pub fn plain(&self) -> String {
        match self {
            SyncError::Offline(_) => "Can't reach the sync server right now. Your changes are kept on this device and will sync when it's reachable.".into(),
            SyncError::ClockSkew { offset_ms } => format!(
                "This device's clock looks wrong: it is {} minutes {} the server's. Set the date and time automatically in this device's settings, then sync again.",
                offset_ms.unsigned_abs() / 60_000,
                if *offset_ms > 0 { "behind" } else { "ahead of" }
            ),
            SyncError::ClockAhead => "This device's clock was set ahead of the real time when some changes were made. Those changes will wait on this device until its clock is right; check the date and time in this device's settings.".into(),
            SyncError::Removed => "This device was removed from the household, so it no longer syncs. What is already here stays on this device.".into(),
            SyncError::RosterHidden => "Another of your devices has a newer list of the household's devices than the sync server is giving this one. To be safe, changes made here stay on this device until that list arrives. If this lasts, the server may be misbehaving: check who runs it.".into(),
            SyncError::NotSetUp => "Sync is not set up on this device.".into(),
            SyncError::Server { code, message, .. } => match code.as_str() {
                "rate_limited" => "Too many tries in a short time. Wait a minute and try again.".into(),
                "no_invitation" => "That invitation has expired or was cancelled. Ask for a new code.".into(),
                "revoked" => "This device was removed from the household.".into(),
                _ => format!("The sync server refused: {message}"),
            },
            SyncError::Crypto(e) => format!("A security check failed, so nothing was changed ({e})."),
            SyncError::Invalid(m) => m.clone(),
            SyncError::Core(e) => format!("Something went wrong on this device: {e}"),
        }
    }

    pub(crate) fn from_response(r: &HttpResponse) -> SyncError {
        match serde_json::from_slice::<ErrorBody>(&r.body) {
            Ok(b) => match b.error.code.as_str() {
                "revoked" => SyncError::Removed,
                "clock_ahead" => SyncError::ClockAhead,
                _ => SyncError::Server {
                    status: r.status,
                    code: b.error.code,
                    message: b.error.message,
                },
            },
            Err(_) => SyncError::Server {
                status: r.status,
                code: "http".into(),
                message: format!("unexpected answer (HTTP {})", r.status),
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Check a server address (D-96): HTTPS, or plain HTTP only to this
/// computer (and, in debug builds only, to the Android emulator's host
/// address and private networks). Returns it without a trailing slash.
pub fn check_server_url(text: &str) -> Result<String> {
    let text = text.trim().trim_end_matches('/');
    let bad = || {
        SyncError::Invalid(
            "That doesn't look like a server address. It should start with https://".into(),
        )
    };
    // A real URL parser (L1): `http://127.0.0.1:1@evil.example` is a
    // user name and the host `evil.example`, and is refused.
    let u = url::Url::parse(text).map_err(|_| bad())?;
    if !u.username().is_empty() || u.password().is_some() {
        return Err(SyncError::Invalid(
            "A server address must not contain a user name or password.".into(),
        ));
    }
    if u.query().is_some() || u.fragment().is_some() {
        return Err(bad());
    }
    let host = u.host().ok_or_else(bad)?;
    match u.scheme() {
        "https" => Ok(text.to_string()),
        "http" => {
            let ip: Option<std::net::IpAddr> = match &host {
                url::Host::Ipv4(a) => Some((*a).into()),
                url::Host::Ipv6(a) => Some((*a).into()),
                url::Host::Domain(_) => None,
            };
            let local = matches!(&host, url::Host::Domain(d) if *d == "localhost")
                || ip.is_some_and(|i| i.is_loopback());
            // Debug builds only: the Android emulator's host and private
            // networks, by address, never by a name that merely starts
            // with "10." (D-96).
            let debug_ok = cfg!(debug_assertions)
                && match ip {
                    Some(std::net::IpAddr::V4(a)) => a.is_private(),
                    Some(std::net::IpAddr::V6(a)) => (a.segments()[0] & 0xfe00) == 0xfc00,
                    None => false,
                };
            if local || debug_ok {
                Ok(text.to_string())
            } else {
                Err(SyncError::Invalid(
                    "The server address must start with https:// so the connection is protected. (Plain http:// only works for a server on this same device.)".into(),
                ))
            }
        }
        _ => Err(bad()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_urls() {
        assert_eq!(
            check_server_url("https://hw.example.org/").unwrap(),
            "https://hw.example.org"
        );
        assert!(check_server_url("https://hw.example.org/sub").is_ok());
        assert!(check_server_url("http://127.0.0.1:8787").is_ok());
        assert!(check_server_url("http://[::1]:8787").is_ok());
        assert!(check_server_url("http://example.org").is_err());
        assert!(check_server_url("ftp://example.org").is_err());
        assert!(check_server_url("example.org").is_err());
        assert!(check_server_url("https://").is_err());
        assert_eq!(
            check_server_url("http://10.0.2.2:8787").is_ok(),
            cfg!(debug_assertions)
        );
    }
}
