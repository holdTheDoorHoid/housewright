//! The local API (D-117): JSON over HTTP, bound to 127.0.0.1 by default,
//! every route but `/api/v1/health` needing `Authorization: Bearer
//! <token>`. The token is compared in constant time. No CORS headers are
//! sent, so a web page in a browser on this computer cannot read answers.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use housewright_api::ApiError;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::Notify;

use crate::Bridge;
use crate::dto::*;

/// Shared between the HTTP handlers and the sync loop.
#[derive(Clone)]
pub struct Shared {
    pub bridge: Arc<Mutex<Bridge>>,
    token_hash: [u8; 32],
    /// Woken after every change, so it reaches the server soon.
    pub changed: Arc<Notify>,
}

impl Shared {
    pub fn new(bridge: Bridge, token: &str) -> Self {
        Self {
            bridge: Arc::new(Mutex::new(bridge)),
            token_hash: Sha256::digest(token.as_bytes()).into(),
            changed: Arc::new(Notify::new()),
        }
    }
}

/// Whether `presented` is the token. Both sides are hashed first, so the
/// comparison takes the same time whatever the length or content.
pub fn token_matches(expected_hash: &[u8; 32], presented: &str) -> bool {
    let h: [u8; 32] = Sha256::digest(presented.as_bytes()).into();
    bool::from(h.ct_eq(expected_hash))
}

/// The token kept in the data folder (`api-token`), made on first use:
/// 32 random bytes as Base32. Only the folder's owner can read it.
pub fn stored_token(dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("data folder: {e}"))?;
    let p = dir.join("api-token");
    if let Ok(t) = std::fs::read_to_string(&p) {
        let t = t.trim().to_string();
        if t.len() >= 32 {
            return Ok(t);
        }
    }
    let raw = housewright_crypto::primitives::random::<32>().map_err(|e| e.to_string())?;
    let t = housewright_crypto::encoding::base32(&raw);
    std::fs::write(&p, format!("{t}\n")).map_err(|e| format!("writing the token: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(t)
}

pub struct Failure(pub ApiError);

impl From<ApiError> for Failure {
    fn from(e: ApiError) -> Self {
        Self(e)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "not_found" => StatusCode::NOT_FOUND,
            "not_paired" | "not_initialized" => StatusCode::CONFLICT,
            "invalid" | "bad_request" => StatusCode::BAD_REQUEST,
            "internal" => StatusCode::INTERNAL_SERVER_ERROR,
            // Sync's own codes: the server could not be reached, refused...
            _ => StatusCode::BAD_GATEWAY,
        };
        (status, Json(json!({ "error": self.0 }))).into_response()
    }
}

type Answer<T> = Result<Json<T>, Failure>;

/// Run `f` on the bridge off the async threads (SQLite and sync block).
async fn with<T: Send + 'static>(
    s: &Shared,
    changes: bool,
    f: impl FnOnce(&mut Bridge) -> Result<T, ApiError> + Send + 'static,
) -> Answer<T> {
    let b = s.bridge.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut g = b.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut g)
    })
    .await
    .map_err(|e| Failure(ApiError::internal(e.to_string())))?;
    if changes && out.is_ok() {
        s.changed.notify_one();
    }
    Ok(Json(out?))
}

async fn auth(State(s): State<Shared>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| token_matches(&s.token_hash, t.trim()));
    if !ok {
        return Failure(ApiError {
            code: "unauthorized".into(),
            message: "The bridge's token is missing or wrong.".into(),
        })
        .into_response();
    }
    next.run(req).await
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    service: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        service: "housewright-bridge",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Bad JSON comes back as our own error shape.
fn body<T>(r: Result<Json<T>, axum::extract::rejection::JsonRejection>) -> Result<T, Failure> {
    r.map(|Json(t)| t)
        .map_err(|e| Failure(ApiError::invalid(e.body_text())))
}

type In<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

pub fn router(s: Shared) -> Router {
    let api = Router::new()
        .route(
            "/api/v1/status",
            get(|State(s): State<Shared>| async move { with(&s, false, |b| b.status()).await }),
        )
        .route(
            "/api/v1/snapshot",
            get(|State(s): State<Shared>| async move { with(&s, false, |b| b.snapshot()).await }),
        )
        .route(
            "/api/v1/tasks",
            get(|State(s): State<Shared>| async move { with(&s, false, |b| b.tasks()).await }),
        )
        .route(
            "/api/v1/items",
            get(|State(s): State<Shared>| async move { with(&s, false, |b| b.items()).await }),
        )
        .route(
            "/api/v1/members",
            get(|State(s): State<Shared>| async move { with(&s, false, |b| b.members()).await }),
        )
        .route(
            "/api/v1/complete",
            post(|State(s): State<Shared>, a: In<CompleteIn>| async move {
                let a = body(a)?;
                with(&s, true, move |b| b.complete(a)).await
            }),
        )
        .route(
            "/api/v1/snooze",
            post(|State(s): State<Shared>, a: In<SnoozeIn>| async move {
                let a = body(a)?;
                with(&s, true, move |b| b.snooze(a)).await
            }),
        )
        .route(
            "/api/v1/readings",
            post(|State(s): State<Shared>, a: In<ReadingIn>| async move {
                let a = body(a)?;
                with(&s, true, move |b| b.reading(a)).await
            }),
        )
        .route(
            "/api/v1/events",
            post(|State(s): State<Shared>, a: In<EventIn>| async move {
                let a = body(a)?;
                with(&s, true, move |b| b.event(a)).await
            }),
        )
        .route(
            "/api/v1/devices/match",
            post(|State(s): State<Shared>, a: In<DevicesIn>| async move {
                let a = body(a)?;
                with(&s, false, move |b| b.match_devices(a.devices)).await
            }),
        )
        .route(
            "/api/v1/devices/import",
            post(|State(s): State<Shared>, a: In<DevicesIn>| async move {
                let a = body(a)?;
                with(&s, true, move |b| b.import_devices(a.devices)).await
            }),
        )
        .route(
            "/api/v1/pairing",
            post(|State(s): State<Shared>, a: In<PairIn>| async move {
                let a = body(a)?;
                with(&s, false, move |b| b.pair_start(a)).await
            })
            .get(|State(s): State<Shared>| async move { with(&s, true, |b| b.pair_poll()).await }),
        )
        .route(
            "/api/v1/pairing/confirm",
            post(|State(s): State<Shared>, a: In<ConfirmIn>| async move {
                let a = body(a)?;
                with(&s, true, move |b| b.pair_confirm(a.matches)).await
            }),
        )
        .route(
            "/api/v1/sync",
            post(|State(s): State<Shared>| async move {
                with(&s, false, |b| {
                    b.sync_quietly();
                    b.status()
                })
                .await
            }),
        )
        .layer(middleware::from_fn_with_state(s.clone(), auth));
    Router::new()
        .route("/api/v1/health", get(health))
        .merge(api)
        .fallback(|| async {
            Failure(ApiError {
                code: "not_found".into(),
                message: "No such address on the bridge.".into(),
            })
        })
        .layer(tower_http::limit::RequestBodyLimitLayer::new(256 * 1024))
        .with_state(s)
}

/// Sync every `every`, and soon after each change.
pub async fn sync_loop(s: Shared, every: std::time::Duration) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(every) => {}
            _ = s.changed.notified() => {
                // Let a burst of changes settle into one sync.
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
        let b = s.bridge.clone();
        let _ = tokio::task::spawn_blocking(move || {
            b.lock().unwrap_or_else(|p| p.into_inner()).sync_quietly();
        })
        .await;
    }
}
