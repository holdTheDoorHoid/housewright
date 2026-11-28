//! `housewright-server`: the sync relay (D-14, D-89 to D-96).
//!
//! It stores and relays **only ciphertext**: households are opaque ids,
//! devices are public keys, operations are encrypted signed batches in
//! arrival order, files are encrypted blobs under keyed-hash names, key
//! envelopes are sealed to one device each, and pairing invitations are
//! sealed with a secret the server never sees. There are no accounts, no
//! e-mail addresses and no passwords: a device is its key, and every
//! request is signed by it ([`housewright_crypto::request`]).
//!
//! What the server can see is listed plainly in `docs/SECURITY.md`.
//!
//! Licensed AGPL-3.0-only.

mod handlers;
mod limit;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use housewright_crypto::request::{H_SERVER_TIME, RequestAuth, verify_request};
use housewright_crypto::roster::DeviceRole;
use housewright_crypto::wire::{ErrorBody, ErrorInfo};
use housewright_crypto::{DeviceId, DevicePublic, MAX_CLOCK_AHEAD_MS, SpaceId};
use rusqlite::{Connection, OptionalExtension, params};

pub use limit::Limiter;

/// The server's clock, in milliseconds. Injected so tests can move it.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

/// Largest JSON request body (operation pushes are split to fit).
pub const MAX_BODY: usize = 4 * 1024 * 1024;
/// Largest encrypted file: a 25 MB PDF (D-72) plus padding and headers.
pub const MAX_BLOB: usize = 26 * 1024 * 1024 + 8192;

#[derive(Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    /// Trust `X-Forwarded-For` for rate limiting (only behind a proxy you run).
    pub behind_proxy: bool,
    /// When set, creating a household needs this token.
    pub registration_token: Option<String>,
    pub max_households: u64,
    /// Bytes one household may store (batches and files).
    pub quota_bytes: u64,
    pub clock: Clock,
}

impl Config {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            behind_proxy: false,
            registration_token: None,
            max_households: 20,
            quota_bytes: 2 * 1024 * 1024 * 1024,
            clock: system_clock(),
        }
    }
}

pub(crate) struct Inner {
    db: Mutex<Connection>,
    blob_dir: PathBuf,
    cfg: Config,
    limiter: Mutex<Limiter>,
}

#[derive(Clone)]
pub struct AppState(pub(crate) Arc<Inner>);

impl AppState {
    pub fn open(cfg: Config) -> Result<Self, String> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(|e| format!("data folder: {e}"))?;
        let blob_dir = cfg.data_dir.join("blobs");
        std::fs::create_dir_all(&blob_dir).map_err(|e| format!("blob folder: {e}"))?;
        let conn = Connection::open(cfg.data_dir.join("housewright-server.db"))
            .map_err(|e| format!("database: {e}"))?;
        init_db(&conn).map_err(|e| format!("database: {e}"))?;
        Ok(AppState(Arc::new(Inner {
            db: Mutex::new(conn),
            blob_dir,
            cfg,
            limiter: Mutex::new(Limiter::default()),
        })))
    }

    pub(crate) fn db(&self) -> MutexGuard<'_, Connection> {
        self.0.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn now(&self) -> u64 {
        (self.0.cfg.clock)()
    }

    pub(crate) fn cfg(&self) -> &Config {
        &self.0.cfg
    }

    pub(crate) fn blob_path(&self, space: SpaceId, name: &str) -> PathBuf {
        self.0.blob_dir.join(space.to_string()).join(name)
    }

    pub fn purge_nonces(&self) {
        let now = self.now();
        let _ = self
            .db()
            .execute("DELETE FROM nonces WHERE expires_ms < ?1", [now as i64]);
    }

    pub(crate) fn limit(&self, bucket: &'static str, ip: &str) -> Result<(), ApiErr> {
        let now = self.now();
        let mut l = self.0.limiter.lock().unwrap_or_else(|e| e.into_inner());
        if l.allow(bucket, ip, now) {
            Ok(())
        } else {
            Err(ApiErr::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "too many attempts; wait a minute and try again",
            ))
        }
    }
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    let _ = conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()));
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS spaces (
             id TEXT PRIMARY KEY, created_ms INTEGER NOT NULL,
             roster_version INTEGER NOT NULL, epoch INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS devices (
             id TEXT PRIMARY KEY, space TEXT NOT NULL, sign BLOB NOT NULL, dh BLOB NOT NULL,
             role TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0,
             added_ms INTEGER NOT NULL, last_seen_ms INTEGER);
         CREATE INDEX IF NOT EXISTS devices_space ON devices(space);
         CREATE TABLE IF NOT EXISTS rosters (
             space TEXT NOT NULL, version INTEGER NOT NULL, data TEXT NOT NULL,
             PRIMARY KEY (space, version));
         CREATE TABLE IF NOT EXISTS envelopes (
             space TEXT NOT NULL, device TEXT NOT NULL, epoch INTEGER NOT NULL, data TEXT NOT NULL,
             PRIMARY KEY (space, device, epoch));
         CREATE TABLE IF NOT EXISTS anchors (space TEXT PRIMARY KEY, data BLOB NOT NULL);
         CREATE TABLE IF NOT EXISTS batches (
             space TEXT NOT NULL, seq INTEGER NOT NULL, batch_id TEXT NOT NULL,
             device TEXT NOT NULL, device_seq INTEGER NOT NULL, received_ms INTEGER NOT NULL,
             data TEXT NOT NULL,
             PRIMARY KEY (space, seq), UNIQUE (space, batch_id), UNIQUE (space, device, device_seq));
         CREATE TABLE IF NOT EXISTS blobs (
             space TEXT NOT NULL, name TEXT NOT NULL, size INTEGER NOT NULL, created_ms INTEGER NOT NULL,
             PRIMARY KEY (space, name));
         CREATE TABLE IF NOT EXISTS invitations (
             id TEXT PRIMARY KEY, space TEXT NOT NULL, created_by TEXT NOT NULL,
             expires_ms INTEGER NOT NULL, offer BLOB NOT NULL, request BLOB, reveal BLOB,
             confirm BLOB, state TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS nonces (
             device TEXT NOT NULL, nonce TEXT NOT NULL, expires_ms INTEGER NOT NULL,
             PRIMARY KEY (device, nonce));",
    )?;
    // Added after the first Phase 6 build.
    let _ = conn.execute("ALTER TABLE invitations ADD COLUMN confirm BLOB", []);
    let _ = conn.execute(
        "ALTER TABLE blobs ADD COLUMN bad INTEGER NOT NULL DEFAULT 0",
        [],
    );
    Ok(())
}

/// An error as the client sees it: a status and `{"error": {...}}`.
#[derive(Debug)]
pub struct ApiErr {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub server_time_ms: Option<u64>,
}

impl ApiErr {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            server_time_ms: None,
        }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid", message)
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "not found")
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }
}

impl From<rusqlite::Error> for ApiErr {
    fn from(e: rusqlite::Error) -> Self {
        // Never echo data; the message names only the kind of failure.
        eprintln!("housewright-server: database error: {e}");
        ApiErr::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "storage error",
        )
    }
}

impl From<housewright_crypto::CryptoError> for ApiErr {
    fn from(e: housewright_crypto::CryptoError) -> Self {
        ApiErr::bad(e.to_string())
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorInfo {
                code: self.code.into(),
                message: self.message,
                server_time_ms: self.server_time_ms,
            },
        };
        (self.status, axum::Json(body)).into_response()
    }
}

/// A device that signed this request.
#[derive(Clone, Debug)]
pub(crate) struct Authed {
    pub id: DeviceId,
    pub role: DeviceRole,
}

pub(crate) struct DeviceRow {
    pub public: DevicePublic,
    pub space: SpaceId,
    pub role: DeviceRole,
    pub revoked: bool,
}

pub(crate) fn load_device(conn: &Connection, id: DeviceId) -> Result<Option<DeviceRow>, ApiErr> {
    type Row = (String, Vec<u8>, Vec<u8>, String, i64);
    let row: Option<Row> = conn
        .query_row(
            "SELECT space, sign, dh, role, revoked FROM devices WHERE id = ?1",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((space, sign, dh, role, revoked)) = row else {
        return Ok(None);
    };
    let corrupt = || {
        ApiErr::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "storage error",
        )
    };
    Ok(Some(DeviceRow {
        public: DevicePublic {
            id,
            sign: sign.try_into().map_err(|_| corrupt())?,
            dh: dh.try_into().map_err(|_| corrupt())?,
        },
        space: space.parse().map_err(|_| corrupt())?,
        role: DeviceRole::parse(&role).ok_or_else(corrupt)?,
        revoked: revoked != 0,
    }))
}

/// Check the request signature, the clock window and the nonce; return
/// the device. `known` supplies the key for a device that is registering
/// itself in this very request (creating a household).
pub(crate) fn authenticate(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
    known: Option<&DevicePublic>,
) -> Result<Option<(RequestAuth, Option<DeviceRow>)>, ApiErr> {
    let get = |n: &str| headers.get(n).and_then(|v| v.to_str().ok());
    if get(housewright_crypto::request::H_DEVICE).is_none() {
        return Ok(None);
    }
    let auth = RequestAuth::from_headers(get).map_err(|_| {
        ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "bad signature headers",
        )
    })?;
    let now = state.now();
    if auth.time_ms.abs_diff(now) > MAX_CLOCK_AHEAD_MS {
        let mut e = ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "clock_skew",
            "this device's clock is more than five minutes from the server's",
        );
        e.server_time_ms = Some(now);
        return Err(e);
    }
    let conn = state.db();
    let row = load_device(&conn, auth.device)?;
    let public = match (&row, known) {
        (Some(r), _) => r.public.clone(),
        (None, Some(k)) if k.id == auth.device => k.clone(),
        (None, _) => {
            return Err(ApiErr::new(
                StatusCode::UNAUTHORIZED,
                "unknown_device",
                "this device is not part of a household on this server",
            ));
        }
    };
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    verify_request(&public, &auth, method.as_str(), path, body).map_err(|_| {
        ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "signature did not verify",
        )
    })?;
    if let Some(r) = &row
        && r.revoked
    {
        return Err(ApiErr::new(
            StatusCode::FORBIDDEN,
            "revoked",
            "this device has been removed from the household",
        ));
    }
    // Replay protection: each nonce once per device within the window.
    // (Expired nonces are deleted by a timer; see `serve`.)
    let fresh = conn.execute(
        "INSERT OR IGNORE INTO nonces (device, nonce, expires_ms) VALUES (?1, ?2, ?3)",
        params![
            auth.device.to_string(),
            housewright_crypto::encoding::hex(&auth.nonce),
            (auth.time_ms + 2 * MAX_CLOCK_AHEAD_MS) as i64
        ],
    )?;
    if fresh == 0 {
        return Err(ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "replay",
            "this request was already received",
        ));
    }
    if row.is_some() {
        conn.execute(
            "UPDATE devices SET last_seen_ms = ?2 WHERE id = ?1 AND (last_seen_ms IS NULL OR last_seen_ms < ?3)",
            params![auth.device.to_string(), now as i64, now.saturating_sub(30_000) as i64],
        )?;
    }
    Ok(Some((auth, row)))
}

/// Authenticate a registered device of `space`.
pub(crate) fn device_of(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
    space: SpaceId,
) -> Result<Authed, ApiErr> {
    let Some((auth, Some(row))) = authenticate(state, method, uri, headers, body, None)? else {
        return Err(ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "sign this request",
        ));
    };
    if row.space != space {
        return Err(ApiErr::forbidden(
            "this device belongs to another household",
        ));
    }
    Ok(Authed {
        id: auth.device,
        role: row.role,
    })
}

async fn server_time(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut res: Response,
) -> Response {
    if let Ok(v) = HeaderValue::from_str(&state.now().to_string()) {
        res.headers_mut().insert(H_SERVER_TIME, v);
    }
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    res
}

/// The HTTP routes, relative to the server's base path.
pub fn router(state: AppState) -> Router {
    use handlers::*;
    let cors = tower_http::cors::CorsLayer::new()
        // Authentication is a signature in headers, never a cookie, so any
        // origin may call (the web app can be served from anywhere).
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any)
        .expose_headers([axum::http::HeaderName::from_static(H_SERVER_TIME)]);
    let blobs = Router::new()
        .route("/v1/s/{space}/blobs/{name}", put(put_blob).get(get_blob))
        .layer(DefaultBodyLimit::max(MAX_BLOB));
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/spaces", post(create_space))
        .route("/v1/s/{space}/roster", get(get_roster).post(update_roster))
        .route("/v1/s/{space}/keys", get(get_keys))
        .route("/v1/s/{space}/ops", get(pull_ops).post(push_ops))
        .route("/v1/s/{space}/blobs/missing", post(missing_blobs))
        .route("/v1/s/{space}/blobs/{name}/bad", post(report_bad_blob))
        .route("/v1/s/{space}/devices", get(list_devices))
        .route("/v1/s/{space}/chain/{device}", get(chain_head))
        .route("/v1/s/{space}/chain/{device}/{seq}", get(chain_batch))
        .route("/v1/s/{space}/invitations", post(create_invitation))
        .route(
            "/v1/s/{space}/invitations/{id}",
            get(invitation_state).delete(cancel_invitation),
        )
        .route(
            "/v1/s/{space}/invitations/{id}/reveal",
            post(reveal_invitation),
        )
        .route("/v1/invitations/{id}", get(view_invitation))
        .route("/v1/invitations/{id}/join", post(join_invitation))
        .route("/v1/invitations/{id}/abort", post(abort_invitation))
        .route("/v1/invitations/{id}/confirm", post(confirm_invitation))
        .route("/v1/recovery", get(recovery))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .merge(blobs)
        .layer(axum::middleware::map_response_with_state(
            state.clone(),
            server_time,
        ))
        .layer(cors)
        .with_state(state)
}

/// Mount the routes under a base path ("" or "/housewright").
pub fn app(state: AppState, base_path: &str) -> Router {
    let base = base_path.trim_end_matches('/');
    if base.is_empty() {
        router(state)
    } else {
        Router::new().nest(base, router(state))
    }
}

/// How the server listens.
pub enum Listen {
    Plain(SocketAddr),
    Tls {
        addr: SocketAddr,
        cert: PathBuf,
        key: PathBuf,
    },
}

/// Run until Ctrl-C or SIGTERM.
pub async fn serve(state: AppState, base_path: &str, listen: Listen) -> Result<(), String> {
    // Expired nonces go once a minute (L5).
    let st = state.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            t.tick().await;
            st.purge_nonces();
        }
    });
    let app = app(state, base_path).into_make_service_with_connect_info::<SocketAddr>();
    let handle = axum_server::Handle::new();
    let h2 = handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        h2.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
    });
    match listen {
        Listen::Plain(addr) => axum_server::bind(addr)
            .handle(handle)
            .serve(app)
            .await
            .map_err(|e| format!("listen on {addr}: {e}")),
        Listen::Tls { addr, cert, key } => {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
                .await
                .map_err(|e| format!("TLS certificate or key: {e}"))?;
            axum_server::bind_rustls(addr, tls)
                .handle(handle)
                .serve(app)
                .await
                .map_err(|e| format!("listen on {addr}: {e}"))
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}

/// The registration token kept in the data folder, made on first use
/// (16 random bytes as Base32). Only the owner of the folder can read it.
pub fn stored_token(data_dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("data folder: {e}"))?;
    let p = data_dir.join("registration-token");
    if let Ok(t) = std::fs::read_to_string(&p) {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let raw = housewright_crypto::primitives::random::<15>().map_err(|e| e.to_string())?;
    let t = housewright_crypto::encoding::group(&housewright_crypto::encoding::base32(&raw), 4);
    std::fs::write(&p, format!("{t}\n")).map_err(|e| format!("writing the token: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(t)
}

/// For tests: the data directory's files, to check they hold no plaintext.
pub fn data_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
    }
    out
}
