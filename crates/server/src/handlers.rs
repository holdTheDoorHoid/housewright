//! The HTTP handlers. Each one authenticates, checks the household and
//! the device's role, and stores or returns ciphertext. None of them can
//! read anything a household stores.

use std::net::SocketAddr;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use housewright_crypto::batch::{MAX_BATCH_CT, SealedBatch, ZERO_HASH, batch_hash, verify_batch};
use housewright_crypto::roster::{
    DeviceRole, RosterBody, SignedRoster, verify_genesis, verify_next,
};
use housewright_crypto::wire::*;
use housewright_crypto::wrap::KeyEnvelope;
use housewright_crypto::{DeviceId, MAX_CLOCK_AHEAD_MS, SpaceId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

use crate::{ApiErr, AppState, Authed, authenticate, device_of, load_device};

type R<T> = Result<T, ApiErr>;

/// SQLite and file work runs off the async worker threads (L5).
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| internal())?
}

/// The client's address, for rate limiting.
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.cfg().behind_proxy
            && let Some(f) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
            && let Some(last) = f.rsplit(',').next()
        {
            return Ok(ClientIp(last.trim().to_string()));
        }
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_else(|| "local".into());
        Ok(ClientIp(ip))
    }
}

fn space_id(s: &str) -> R<SpaceId> {
    s.parse().map_err(|_| ApiErr::not_found())
}

fn json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> R<T> {
    serde_json::from_slice(body).map_err(|e| ApiErr::bad(format!("bad request body: {e}")))
}

fn need_manager(a: &Authed) -> R<()> {
    if a.role.can_manage() {
        Ok(())
    } else {
        Err(ApiErr::forbidden("only an owner can do this"))
    }
}

fn latest_roster(conn: &Connection, space: SpaceId) -> R<Option<SignedRoster>> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM rosters WHERE space = ?1 ORDER BY version DESC LIMIT 1",
            [space.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    data.map(|d| serde_json::from_str(&d).map_err(|_| internal()))
        .transpose()
}

fn internal() -> ApiErr {
    ApiErr::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "storage error",
    )
}

fn space_epoch(conn: &Connection, space: SpaceId) -> R<u32> {
    conn.query_row(
        "SELECT epoch FROM spaces WHERE id = ?1",
        [space.to_string()],
        |r| r.get::<_, u32>(0),
    )
    .optional()?
    .ok_or_else(ApiErr::not_found)
}

pub async fn health(State(s): State<AppState>) -> Json<Health> {
    Json(Health {
        ok: true,
        version: env!("CARGO_PKG_VERSION").into(),
        time_ms: s.now(),
        registration_open: s.cfg().registration_token.is_none(),
    })
}

fn put_envelope(conn: &Connection, e: &KeyEnvelope) -> R<()> {
    conn.execute(
        "INSERT INTO envelopes (space, device, epoch, data) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(space, device, epoch) DO UPDATE SET data = excluded.data",
        params![
            e.space.to_string(),
            e.recipient.to_string(),
            e.epoch,
            serde_json::to_string(e).map_err(|_| internal())?
        ],
    )?;
    Ok(())
}

fn insert_device(
    conn: &Connection,
    space: SpaceId,
    e: &housewright_crypto::roster::RosterEntry,
    now: u64,
) -> R<()> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO devices (id, space, sign, dh, role, added_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            e.device.id.to_string(),
            space.to_string(),
            e.device.sign.to_vec(),
            e.device.dh.to_vec(),
            e.role.as_str(),
            now as i64
        ],
    )?;
    if n == 0 {
        // Already here (a roster version sent again after a server
        // restore): fine if it is the same device of the same household.
        let same: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM devices WHERE id = ?1 AND space = ?2 AND sign = ?3)",
            params![
                e.device.id.to_string(),
                space.to_string(),
                e.device.sign.to_vec()
            ],
            |r| r.get(0),
        )?;
        if !same {
            return Err(ApiErr::conflict("that device is already registered"));
        }
    }
    Ok(())
}

/// `POST /v1/spaces`: a new household. Signed by its founding device.
pub async fn create_space(
    State(s): State<AppState>,
    ip: ClientIp,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    blocking(move || create_space_sync(State(s), ip, method, uri, headers, body)).await
}

fn create_space_sync(
    State(s): State<AppState>,
    ip: ClientIp,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    s.limit("create", &ip.0)?;
    let req: CreateSpace = json(&body)?;
    if let Some(token) = &s.cfg().registration_token
        && !bool::from(subtle::ConstantTimeEq::ct_eq(
            req.registration_token.as_deref().unwrap_or("").as_bytes(),
            token.as_bytes(),
        ))
    {
        return Err(ApiErr::forbidden(
            "this server needs a registration token to add a household",
        ));
    }
    let genesis: RosterBody = verify_genesis(&req.roster)?;
    if genesis.space != req.space {
        return Err(ApiErr::bad("roster is for another household"));
    }
    let founder = genesis
        .find(req.roster.signer)
        .ok_or_else(|| ApiErr::bad("founder missing"))?
        .device
        .clone();
    let Some((auth, None)) = authenticate(&s, &method, &uri, &headers, &body, Some(&founder))?
    else {
        return Err(ApiErr::conflict("this device is already registered"));
    };
    if auth.device != founder.id {
        return Err(ApiErr::forbidden("must be signed by the founding device"));
    }
    for e in &genesis.devices {
        if !req.envelopes.iter().any(|v| {
            v.recipient == e.device.id
                && v.epoch == 0
                && v.space == req.space
                && v.sender == founder.id
        }) {
            return Err(ApiErr::bad("an envelope is missing"));
        }
    }
    if req.envelopes.len() != genesis.devices.len()
        || req.anchor.is_empty()
        || req.anchor.len() > 4096
    {
        return Err(ApiErr::bad("bad envelopes or anchor"));
    }
    let now = s.now();
    let mut conn = s.db();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM spaces", [], |r| r.get(0))?;
    if count as u64 >= s.cfg().max_households {
        return Err(ApiErr::forbidden("this server is full; ask its owner"));
    }
    let tx = conn.transaction()?;
    let added = tx.execute(
        "INSERT OR IGNORE INTO spaces (id, created_ms, roster_version, epoch) VALUES (?1, ?2, 1, 0)",
        params![req.space.to_string(), now as i64],
    )?;
    if added == 0 {
        return Err(ApiErr::conflict("that household already exists"));
    }
    tx.execute(
        "INSERT INTO rosters (space, version, data) VALUES (?1, 1, ?2)",
        params![
            req.space.to_string(),
            serde_json::to_string(&req.roster).map_err(|_| internal())?
        ],
    )?;
    for e in &genesis.devices {
        insert_device(&tx, req.space, e, now)?;
    }
    for e in &req.envelopes {
        put_envelope(&tx, e)?;
    }
    tx.execute(
        "INSERT INTO anchors (space, data) VALUES (?1, ?2)",
        params![req.space.to_string(), req.anchor],
    )?;
    tx.commit()?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct After {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    limit: Option<u64>,
}

pub async fn get_roster(
    State(s): State<AppState>,
    Path(space): Path<String>,
    Query(q): Query<After>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<RosterList>> {
    let space = space_id(&space)?;
    device_of(&s, &method, &uri, &headers, &body, space)?;
    let conn = s.db();
    let mut stmt = conn
        .prepare("SELECT data FROM rosters WHERE space = ?1 AND version > ?2 ORDER BY version")?;
    let rows = stmt.query_map(params![space.to_string(), q.after as i64], |r| {
        r.get::<_, String>(0)
    })?;
    let mut rosters = Vec::new();
    for r in rows {
        rosters.push(serde_json::from_str(&r?).map_err(|_| internal())?);
    }
    Ok(Json(RosterList { rosters }))
}

/// `POST /v1/s/{space}/roster`: add a device, remove devices (with a new
/// epoch), or replace the recovery key. The server checks the new version
/// exactly as clients do, then updates its device table to match.
pub async fn update_roster(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    blocking(move || update_roster_sync(State(s), Path(space), method, uri, headers, body)).await
}

fn update_roster_sync(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    need_manager(&me)?;
    let req: UpdateRoster = json(&body)?;
    // Normally the signer sends it. After a server restore an owner's
    // device may send again versions another owner signed (D-107); they
    // are checked exactly the same way.
    let now = s.now();
    let mut conn = s.db();
    let prev_signed = latest_roster(&conn, space)?.ok_or_else(ApiErr::not_found)?;
    let prev = prev_signed.parse().map_err(|_| internal())?;
    let next = match verify_next(&prev_signed, &prev, &req.roster) {
        Ok(n) => n,
        Err(e) => {
            let newer = req
                .roster
                .parse()
                .map(|b| b.version <= prev.version)
                .unwrap_or(false);
            return Err(if newer {
                ApiErr::conflict("the household changed meanwhile; fetch and try again")
            } else {
                ApiErr::bad(e.to_string())
            });
        }
    };
    let added: Vec<_> = next
        .devices
        .iter()
        .filter(|e| prev.find(e.device.id).is_none())
        .collect();
    let rotated = next.epoch != prev.epoch;
    for e in &req.envelopes {
        if e.space != space
            || e.epoch != next.epoch
            || e.sender != me.id
            || next.find(e.recipient).is_none()
        {
            return Err(ApiErr::bad("an envelope does not fit this roster"));
        }
    }
    let needs: Vec<DeviceId> = if rotated {
        next.devices.iter().map(|e| e.device.id).collect()
    } else {
        added.iter().map(|e| e.device.id).collect()
    };
    for id in &needs {
        if !req.envelopes.iter().any(|e| e.recipient == *id) {
            return Err(ApiErr::bad("an envelope is missing"));
        }
    }
    // A removal must name the device's real last batch (L3a): if it has
    // sent more since the remover last synced, the remover must catch up.
    // Not for versions sent again after a restore (D-114): the server lost
    // what it would compare against, and the signed last_seq/last_hash
    // already bind what every device accepts.
    for r in &next.removed {
        if prev.find_removed(r.entry.device.id).is_some() {
            continue;
        }
        let stored: Option<(i64, String)> = conn
            .query_row(
                "SELECT device_seq, data FROM batches WHERE space = ?1 AND device = ?2 ORDER BY device_seq DESC LIMIT 1",
                params![space.to_string(), r.entry.device.id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (seq, hash) = match stored {
            Some((seq, data)) => {
                let b: SealedBatch = serde_json::from_str(&data).map_err(|_| internal())?;
                (seq as u64, batch_hash(&b).map_err(|_| internal())?)
            }
            None => (0, ZERO_HASH.to_string()),
        };
        // Re-sent after a restore: the server may have lost the device's
        // later batches (fewer here is fine), but if it holds *more* than
        // the removal names, the device sent changes since: refused either
        // way, so `restore` cannot reopen the race (L3a).
        let stale = if req.restore {
            seq > r.last_seq
        } else {
            seq != r.last_seq || hash != r.last_hash
        };
        if stale {
            return Err(ApiErr::new(
                StatusCode::CONFLICT,
                "removal_stale",
                "that device sent changes since this device last synced; sync and try again",
            ));
        }
    }
    let recovery_changed =
        prev.recovery().map(|r| r.device.id) != next.recovery().map(|r| r.device.id);
    if recovery_changed != req.anchor.is_some() {
        return Err(ApiErr::bad(
            "a new recovery key needs a new anchor, and only then",
        ));
    }
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO rosters (space, version, data) VALUES (?1, ?2, ?3)",
        params![
            space.to_string(),
            next.version as i64,
            serde_json::to_string(&req.roster).map_err(|_| internal())?
        ],
    )?;
    tx.execute(
        "UPDATE spaces SET roster_version = ?2, epoch = ?3 WHERE id = ?1",
        params![space.to_string(), next.version as i64, next.epoch],
    )?;
    for e in &added {
        insert_device(&tx, space, e, now)?;
    }
    for r in &next.removed {
        if prev.find_removed(r.entry.device.id).is_none() {
            tx.execute(
                "UPDATE devices SET revoked = 1 WHERE id = ?1",
                [r.entry.device.id.to_string()],
            )?;
            tx.execute(
                "DELETE FROM nonces WHERE device = ?1",
                [r.entry.device.id.to_string()],
            )?;
        }
    }
    for e in &req.envelopes {
        put_envelope(&tx, e)?;
    }
    if let Some(a) = &req.anchor {
        if a.is_empty() || a.len() > 4096 {
            return Err(ApiErr::bad("bad anchor"));
        }
        tx.execute(
            "INSERT INTO anchors (space, data) VALUES (?1, ?2) ON CONFLICT(space) DO UPDATE SET data = excluded.data",
            params![space.to_string(), a],
        )?;
    }
    if let Some(inv) = &req.invitation {
        // Pairing finished: keep only the state, drop the sealed messages.
        tx.execute(
            "UPDATE invitations SET state = 'accepted', offer = X'', request = NULL, reveal = NULL, confirm = NULL
             WHERE id = ?1 AND space = ?2",
            params![inv, space.to_string()],
        )?;
    }
    tx.commit()?;
    Ok(StatusCode::OK)
}

pub async fn get_keys(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<KeyList>> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    let conn = s.db();
    let mut stmt =
        conn.prepare("SELECT data FROM envelopes WHERE space = ?1 AND device = ?2 ORDER BY epoch")?;
    let rows = stmt.query_map(params![space.to_string(), me.id.to_string()], |r| {
        r.get::<_, String>(0)
    })?;
    let mut envelopes = Vec::new();
    for r in rows {
        envelopes.push(serde_json::from_str(&r?).map_err(|_| internal())?);
    }
    Ok(Json(KeyList { envelopes }))
}

pub async fn push_ops(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<PushResult>> {
    blocking(move || push_ops_sync(State(s), Path(space), method, uri, headers, body)).await
}

fn push_ops_sync(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<PushResult>> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    if me.role == DeviceRole::Recovery {
        return Err(ApiErr::forbidden("the recovery key does not write changes"));
    }
    let req: PushOps = json(&body)?;
    let now = s.now();
    let mut conn = s.db();
    let epoch = space_epoch(&conn, space)?;
    let public = load_device(&conn, me.id)?.ok_or_else(internal)?.public;
    for b in &req.batches {
        let h = &b.header;
        if h.space != space || h.device != me.id {
            return Err(ApiErr::forbidden(
                "a batch names another household or device",
            ));
        }
        if h.epoch > epoch {
            // Usually a server restored from a backup that has not yet been
            // given back its newer roster: the device waits (D-114).
            return Err(ApiErr::new(
                StatusCode::CONFLICT,
                "epoch_ahead",
                "the server has not caught up with the household's newest keys yet",
            ));
        }
        if b.ct.len() > MAX_BATCH_CT {
            return Err(ApiErr::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "a batch is larger than the server accepts",
            ));
        }
        verify_batch(b, &public).map_err(|_| ApiErr::bad("a batch signature did not verify"))?;
        if h.max_wall_ms > now + MAX_CLOCK_AHEAD_MS {
            let mut e = ApiErr::new(
                StatusCode::CONFLICT,
                "clock_ahead",
                "changes were made while this device's clock was ahead of the real time",
            );
            e.server_time_ms = Some(now);
            return Err(e);
        }
    }
    let tx = conn.transaction()?;
    let mut accepted = Vec::new();
    for b in &req.batches {
        let h = &b.header;
        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT seq, device FROM batches WHERE space = ?1 AND batch_id = ?2",
                params![space.to_string(), h.batch_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((seq, dev)) = existing {
            if dev != me.id.to_string() {
                return Err(ApiErr::conflict("batch id already used"));
            }
            accepted.push(Accepted {
                batch_id: h.batch_id.clone(),
                seq: seq as u64,
            });
            continue;
        }
        let reused: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM batches WHERE space = ?1 AND device = ?2 AND device_seq = ?3)",
            params![space.to_string(), me.id.to_string(), h.device_seq as i64],
            |r| r.get(0),
        )?;
        if reused {
            return Err(ApiErr::new(
                StatusCode::CONFLICT,
                "seq_reused",
                "this device already sent a different batch with that number",
            ));
        }
        // Each device's batches form a hash chain (D-99).
        let prev_hash =
            if h.device_seq <= 1 {
                Some(ZERO_HASH.to_string())
            } else {
                let data: Option<String> = tx
                .query_row(
                    "SELECT data FROM batches WHERE space = ?1 AND device = ?2 AND device_seq = ?3",
                    params![space.to_string(), me.id.to_string(), (h.device_seq - 1) as i64],
                    |r| r.get(0),
                )
                .optional()?;
                match data {
                    Some(d) => {
                        let pb: SealedBatch = serde_json::from_str(&d).map_err(|_| internal())?;
                        Some(batch_hash(&pb).map_err(|_| internal())?)
                    }
                    None => None,
                }
            };
        if prev_hash.as_deref() != Some(h.prev.as_str()) {
            return Err(ApiErr::new(
                StatusCode::CONFLICT,
                "chain",
                "this batch does not follow the device's previous batch",
            ));
        }
        let data = serde_json::to_string(b).map_err(|_| internal())?;
        if usage(&tx, space)? + data.len() as u64 > s.cfg().quota_bytes {
            return Err(ApiErr::new(
                StatusCode::INSUFFICIENT_STORAGE,
                "quota",
                "this household has used all the space this server allows",
            ));
        }
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM batches WHERE space = ?1",
            [space.to_string()],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                space.to_string(),
                seq,
                h.batch_id,
                me.id.to_string(),
                h.device_seq as i64,
                now as i64,
                data
            ],
        )?;
        accepted.push(Accepted {
            batch_id: h.batch_id.clone(),
            seq: seq as u64,
        });
    }
    tx.commit()?;
    Ok(Json(PushResult { accepted }))
}

pub async fn pull_ops(
    State(s): State<AppState>,
    Path(space): Path<String>,
    Query(q): Query<After>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<PullOps>> {
    blocking(move || pull_ops_sync(State(s), Path(space), Query(q), method, uri, headers, body))
        .await
}

fn pull_ops_sync(
    State(s): State<AppState>,
    Path(space): Path<String>,
    Query(q): Query<After>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<PullOps>> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    if me.role == DeviceRole::Recovery {
        return Err(ApiErr::forbidden("the recovery key does not read changes"));
    }
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let conn = s.db();
    let mut stmt = conn.prepare(
        "SELECT seq, received_ms, data, device, device_seq FROM batches WHERE space = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![space.to_string(), q.after as i64, (limit + 1) as i64],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        },
    )?;
    // Pages stop at a byte budget as well as a count (D-100), so no
    // answer is ever too large for a client to read.
    let mut batches = Vec::new();
    let mut bytes = 0usize;
    let mut more = false;
    for r in rows {
        let (seq, received, data, device, device_seq) = r?;
        if batches.len() as u64 >= limit || (bytes > 0 && bytes + data.len() > PAGE_BYTES) {
            more = true;
            break;
        }
        if data.len() > MAX_STORED_BATCH {
            batches.push(Pulled {
                seq: seq as u64,
                received_ms: received as u64,
                batch: None,
                oversize: true,
                device: device.parse().ok(),
                device_seq: device_seq as u64,
            });
            continue;
        }
        bytes += data.len();
        let batch: SealedBatch = serde_json::from_str(&data).map_err(|_| internal())?;
        batches.push(Pulled {
            seq: seq as u64,
            received_ms: received as u64,
            batch: Some(batch),
            oversize: false,
            device: None,
            device_seq: 0,
        });
    }
    drop(stmt);
    let head: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM batches WHERE space = ?1",
        [space.to_string()],
        |r| r.get(0),
    )?;
    let roster_version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM rosters WHERE space = ?1",
        [space.to_string()],
        |r| r.get(0),
    )?;
    Ok(Json(PullOps {
        batches,
        more,
        head: head as u64,
        roster_version: roster_version as u64,
    }))
}

/// `GET /v1/s/{space}/chain/{device}/{seq}`: one batch by its device's
/// own number, to fill a gap (D-107).
pub async fn chain_batch(
    State(s): State<AppState>,
    Path((space, device, dseq)): Path<(String, String, u64)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<OneBatch>> {
    blocking(move || {
        let space = space_id(&space)?;
        device_of(&s, &method, &uri, &headers, &body, space)?;
        let conn = s.db();
        let row: Option<(i64, i64, String)> = conn
            .query_row(
                "SELECT seq, received_ms, data FROM batches WHERE space = ?1 AND device = ?2 AND device_seq = ?3",
                params![space.to_string(), device, dseq as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (seq, received, data) = row.ok_or_else(ApiErr::not_found)?;
        if data.len() > MAX_STORED_BATCH {
            return Err(ApiErr::new(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "that batch is too large to send"));
        }
        let batch: SealedBatch = serde_json::from_str(&data).map_err(|_| internal())?;
        Ok(Json(OneBatch {
            batch: Pulled {
                seq: seq as u64,
                received_ms: received as u64,
                batch: Some(batch),
                oversize: false,
                device: None,
                device_seq: 0,
            },
        }))
    })
    .await
}

/// `GET /v1/s/{space}/chain/{device}`: the newest number held for a device.
pub async fn chain_head(
    State(s): State<AppState>,
    Path((space, device)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<ChainHead>> {
    blocking(move || {
        let space = space_id(&space)?;
        device_of(&s, &method, &uri, &headers, &body, space)?;
        let conn = s.db();
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(device_seq), 0) FROM batches WHERE space = ?1 AND device = ?2",
            params![space.to_string(), device],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT device_seq FROM batches WHERE space = ?1 AND device = ?2 ORDER BY device_seq",
        )?;
        let have: std::collections::BTreeSet<i64> = stmt
            .query_map(params![space.to_string(), device], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let missing = (1..seq)
            .filter(|n| !have.contains(n))
            .take(1000)
            .map(|n| n as u64)
            .collect();
        Ok(Json(ChainHead {
            seq: seq as u64,
            missing,
        }))
    })
    .await
}

/// Bytes of batch data per pull page.
const PAGE_BYTES: usize = 8 * 1024 * 1024;
/// A stored batch's JSON can be at most this (base64 of the largest
/// ciphertext plus the header); anything bigger is never sent.
const MAX_STORED_BATCH: usize = MAX_BATCH_CT * 4 / 3 + 4096;

/// Bytes a household stores (batches and files).
fn usage(conn: &Connection, space: SpaceId) -> R<u64> {
    let b: i64 = conn.query_row(
        "SELECT COALESCE(SUM(length(data)), 0) FROM batches WHERE space = ?1",
        [space.to_string()],
        |r| r.get(0),
    )?;
    let f: i64 = conn.query_row(
        "SELECT COALESCE(SUM(size), 0) FROM blob_copies WHERE space = ?1",
        [space.to_string()],
        |r| r.get(0),
    )?;
    Ok((b + f) as u64)
}

fn blob_name_ok(n: &str) -> bool {
    n.len() == 32
        && n.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub async fn missing_blobs(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<BlobNames>> {
    blocking(move || missing_blobs_sync(State(s), Path(space), method, uri, headers, body)).await
}

fn missing_blobs_sync(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<BlobNames>> {
    let space = space_id(&space)?;
    device_of(&s, &method, &uri, &headers, &body, space)?;
    let req: BlobNames = json(&body)?;
    if req.names.len() > 2000 || !req.names.iter().all(|n| blob_name_ok(n)) {
        return Err(ApiErr::bad("bad file names"));
    }
    let conn = s.db();
    let mut names = Vec::new();
    for n in req.names {
        // Missing, or reported bad by a device (D-108): a device that has
        // the real file sends it again.
        let have: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM blobs WHERE space = ?1 AND name = ?2 AND bad = 0)",
            params![space.to_string(), n],
            |r| r.get(0),
        )?;
        if !have {
            names.push(n);
        }
    }
    Ok(Json(BlobNames { names }))
}

pub async fn put_blob(
    State(s): State<AppState>,
    Path(path): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    blocking(move || put_blob_sync(State(s), Path(path), method, uri, headers, body)).await
}

fn put_blob_sync(
    State(s): State<AppState>,
    Path((space, name)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    if me.role == DeviceRole::Recovery {
        return Err(ApiErr::forbidden("the recovery key does not write files"));
    }
    if !blob_name_ok(&name) || body.len() < 32 {
        return Err(ApiErr::bad("bad file"));
    }
    // One copy per uploading device, never overwritten by anyone else
    // (D-113): a member cannot replace another device's copy, and a
    // receiver tries every copy against the hash in the signed record.
    let path = s.blob_copy_path(space, &name, me.id);
    let dir = path.parent().expect("has a parent");
    let io = |e: std::io::Error| {
        eprintln!("housewright-server: file storage error: {}", e.kind());
        internal()
    };
    // Check and write under the database lock, so parallel uploads cannot
    // both pass the quota; a device replacing its own copy is charged the
    // difference (Q2).
    let conn = s.db();
    let old: i64 = conn
        .query_row(
            "SELECT size FROM blob_copies WHERE space = ?1 AND name = ?2 AND device = ?3",
            params![space.to_string(), name, me.id.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    // (Shrinking or keeping the size is always allowed.)
    if body.len() as i64 > old
        && (usage(&conn, space)? as i64 - old + body.len() as i64) as u64 > s.cfg().quota_bytes
    {
        return Err(ApiErr::new(
            StatusCode::INSUFFICIENT_STORAGE,
            "quota",
            "this household has used all the space this server allows",
        ));
    }
    std::fs::create_dir_all(dir).map_err(io)?;
    // A unique temporary name: two uploads at once never mix (L2).
    let rnd = housewright_crypto::encoding::hex(
        &housewright_crypto::primitives::random::<8>().map_err(|_| internal())?,
    );
    let tmp = dir.join(format!(".{name}.{rnd}.tmp"));
    std::fs::write(&tmp, &body).map_err(io)?;
    std::fs::rename(&tmp, &path).map_err(io)?;
    conn.execute(
        "INSERT INTO blob_copies (space, name, device, size, created_ms) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(space, name, device) DO UPDATE SET size = excluded.size",
        params![space.to_string(), name, me.id.to_string(), body.len() as i64, s.now() as i64],
    )?;
    conn.execute(
        "INSERT INTO blobs (space, name, size, created_ms) VALUES (?1, ?2, 0, ?3)
         ON CONFLICT(space, name) DO UPDATE SET bad = 0",
        params![space.to_string(), name, s.now() as i64],
    )?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct From {
    #[serde(default)]
    from: Option<String>,
}

/// `GET /v1/s/{space}/blobs/{name}[?from=<device>]`: one copy (the oldest
/// by default), with `hw-copies` listing every uploader's copy.
pub async fn get_blob(
    State(s): State<AppState>,
    Path((space, name)): Path<(String, String)>,
    Query(q): Query<From>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Response> {
    let space = space_id(&space)?;
    device_of(&s, &method, &uri, &headers, &body, space)?;
    if !blob_name_ok(&name) {
        return Err(ApiErr::not_found());
    }
    let copies: Vec<String> = {
        let conn = s.db();
        let mut stmt = conn.prepare(
            "SELECT device FROM blob_copies WHERE space = ?1 AND name = ?2 ORDER BY created_ms, device",
        )?;
        let rows = stmt.query_map(params![space.to_string(), name], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let pick = match &q.from {
        Some(d) if copies.contains(d) => d.clone(),
        Some(_) => return Err(ApiErr::not_found()),
        None => copies.first().cloned().ok_or_else(ApiErr::not_found)?,
    };
    let device: DeviceId = pick.parse().map_err(|_| internal())?;
    // Streamed from the file, not read into memory (L5).
    match tokio::fs::File::open(s.blob_copy_path(space, &name, device)).await {
        Ok(f) => {
            let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(f));
            Ok((
                [
                    ("content-type", "application/octet-stream".to_string()),
                    ("hw-copies", copies.join(",")),
                ],
                body,
            )
                .into_response())
        }
        Err(_) => Err(ApiErr::not_found()),
    }
}

/// `POST /v1/s/{space}/blobs/{name}/bad`: no copy of this file decrypted
/// to its hash. Nothing is deleted or replaced: the name is marked, so a
/// device holding the real file adds its own copy (D-108, D-113).
pub async fn report_bad_blob(
    State(s): State<AppState>,
    Path((space, name)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    blocking(move || {
        let space = space_id(&space)?;
        let me = device_of(&s, &method, &uri, &headers, &body, space)?;
        if me.role == DeviceRole::Recovery || !blob_name_ok(&name) {
            return Err(ApiErr::forbidden("not allowed"));
        }
        s.db().execute(
            "UPDATE blobs SET bad = 1 WHERE space = ?1 AND name = ?2",
            params![space.to_string(), name],
        )?;
        Ok(StatusCode::OK)
    })
    .await
}

pub async fn list_devices(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<DeviceList>> {
    let space = space_id(&space)?;
    device_of(&s, &method, &uri, &headers, &body, space)?;
    let conn = s.db();
    let mut stmt = conn.prepare(
        "SELECT id, role, added_ms, last_seen_ms FROM devices WHERE space = ?1 AND revoked = 0 ORDER BY added_ms",
    )?;
    let rows = stmt.query_map([space.to_string()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })?;
    let mut devices = Vec::new();
    for r in rows {
        let (id, role, added, seen) = r?;
        devices.push(DeviceSeen {
            id: id.parse().map_err(|_| internal())?,
            role: DeviceRole::parse(&role).ok_or_else(internal)?,
            added_ms: added as u64,
            last_seen_ms: seen.map(|v| v as u64),
        });
    }
    Ok(Json(DeviceList { devices }))
}

/// Invitations live at most 30 minutes; at most five open per household.
const INVITATION_MAX_MS: u64 = 30 * 60 * 1000;

fn invitation_id_ok(id: &str) -> bool {
    id.len() == 10
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn purge_invitations(conn: &Connection, now: u64) -> R<()> {
    conn.execute(
        "DELETE FROM invitations WHERE expires_ms < ?1",
        [now as i64],
    )?;
    Ok(())
}

pub async fn create_invitation(
    State(s): State<AppState>,
    Path(space): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    if me.role != DeviceRole::Owner {
        return Err(ApiErr::forbidden("only an owner's device can add devices"));
    }
    let req: CreateInvitation = json(&body)?;
    let now = s.now();
    if !invitation_id_ok(&req.id)
        || req.offer.len() > 8192
        || req.expires_ms > now + INVITATION_MAX_MS
        || req.expires_ms <= now
    {
        return Err(ApiErr::bad("bad invitation"));
    }
    let conn = s.db();
    purge_invitations(&conn, now)?;
    let open: i64 = conn.query_row(
        "SELECT COUNT(*) FROM invitations WHERE space = ?1 AND state NOT IN ('accepted', 'aborted')",
        [space.to_string()],
        |r| r.get(0),
    )?;
    if open >= 5 {
        return Err(ApiErr::conflict(
            "too many invitations are open; finish or cancel one",
        ));
    }
    let n = conn.execute(
        "INSERT OR IGNORE INTO invitations (id, space, created_by, expires_ms, offer, state) VALUES (?1, ?2, ?3, ?4, ?5, 'open')",
        params![req.id, space.to_string(), me.id.to_string(), req.expires_ms as i64, req.offer],
    )?;
    if n == 0 {
        return Err(ApiErr::conflict("invitation id in use"));
    }
    Ok(StatusCode::CREATED)
}

struct InvRow {
    space: String,
    offer: Vec<u8>,
    request: Option<Vec<u8>>,
    reveal: Option<Vec<u8>>,
    confirm: Option<Vec<u8>>,
    state: String,
}

fn load_invitation(conn: &Connection, id: &str, now: u64) -> R<InvRow> {
    if !invitation_id_ok(id) {
        return Err(ApiErr::not_found());
    }
    purge_invitations(conn, now)?;
    conn.query_row(
        "SELECT space, offer, request, reveal, confirm, state FROM invitations WHERE id = ?1",
        [id],
        |r| {
            Ok(InvRow {
                space: r.get(0)?,
                offer: r.get(1)?,
                request: r.get(2)?,
                reveal: r.get(3)?,
                confirm: r.get(4)?,
                state: r.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| {
        ApiErr::new(
            StatusCode::NOT_FOUND,
            "no_invitation",
            "no such invitation, or it has expired",
        )
    })
}

pub async fn view_invitation(
    State(s): State<AppState>,
    ip: ClientIp,
    Path(id): Path<String>,
) -> R<Json<InvitationView>> {
    s.limit("pair", &ip.0)?;
    let conn = s.db();
    let inv = load_invitation(&conn, &id, s.now())?;
    Ok(Json(InvitationView {
        offer: inv.offer,
        reveal: inv.reveal,
        state: inv.state,
    }))
}

pub async fn join_invitation(
    State(s): State<AppState>,
    ip: ClientIp,
    Path(id): Path<String>,
    body: Bytes,
) -> R<StatusCode> {
    s.limit("pair", &ip.0)?;
    let req: JoinRequest = json(&body)?;
    if req.request.len() > 8192 {
        return Err(ApiErr::bad("request too large"));
    }
    let conn = s.db();
    let inv = load_invitation(&conn, &id, s.now())?;
    if inv.state != "open" {
        return Err(ApiErr::conflict(
            "someone has already answered this invitation",
        ));
    }
    conn.execute(
        "UPDATE invitations SET request = ?2, state = 'joined' WHERE id = ?1 AND state = 'open'",
        params![id, req.request],
    )?;
    Ok(StatusCode::OK)
}

/// The new device says the numbers match (sealed; D-102). The owner's
/// device only finishes after this.
pub async fn confirm_invitation(
    State(s): State<AppState>,
    ip: ClientIp,
    Path(id): Path<String>,
    body: Bytes,
) -> R<StatusCode> {
    s.limit("pair", &ip.0)?;
    let req: JoinConfirm = json(&body)?;
    if req.confirm.len() > 1024 {
        return Err(ApiErr::bad("too large"));
    }
    let conn = s.db();
    let inv = load_invitation(&conn, &id, s.now())?;
    if inv.state != "revealed" {
        return Err(ApiErr::conflict("not ready to confirm"));
    }
    conn.execute(
        "UPDATE invitations SET confirm = ?2, state = 'confirmed' WHERE id = ?1 AND state = 'revealed'",
        params![id, req.confirm],
    )?;
    Ok(StatusCode::OK)
}

pub async fn abort_invitation(
    State(s): State<AppState>,
    ip: ClientIp,
    Path(id): Path<String>,
) -> R<StatusCode> {
    s.limit("pair", &ip.0)?;
    let conn = s.db();
    let inv = load_invitation(&conn, &id, s.now())?;
    if inv.state == "accepted" {
        return Err(ApiErr::conflict("already finished"));
    }
    conn.execute(
        "UPDATE invitations SET state = 'aborted' WHERE id = ?1",
        [id],
    )?;
    Ok(StatusCode::OK)
}

pub async fn invitation_state(
    State(s): State<AppState>,
    Path((space, id)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<InvitationState>> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    need_manager(&me)?;
    let conn = s.db();
    let inv = load_invitation(&conn, &id, s.now())?;
    if inv.space != space.to_string() {
        return Err(ApiErr::not_found());
    }
    Ok(Json(InvitationState {
        state: inv.state,
        request: inv.request,
        confirm: inv.confirm,
    }))
}

pub async fn reveal_invitation(
    State(s): State<AppState>,
    Path((space, id)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    need_manager(&me)?;
    let req: Reveal = json(&body)?;
    if req.reveal.len() > 4096 {
        return Err(ApiErr::bad("too large"));
    }
    let conn = s.db();
    let inv = load_invitation(&conn, &id, s.now())?;
    if inv.space != space.to_string() {
        return Err(ApiErr::not_found());
    }
    if inv.state != "joined" {
        return Err(ApiErr::conflict("nobody has answered this invitation yet"));
    }
    conn.execute(
        "UPDATE invitations SET reveal = ?2, state = 'revealed' WHERE id = ?1",
        params![id, req.reveal],
    )?;
    Ok(StatusCode::OK)
}

pub async fn cancel_invitation(
    State(s): State<AppState>,
    Path((space, id)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<StatusCode> {
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    need_manager(&me)?;
    s.db().execute(
        "DELETE FROM invitations WHERE id = ?1 AND space = ?2",
        params![id, space.to_string()],
    )?;
    Ok(StatusCode::OK)
}

/// `GET /v1/recovery`: where the recovery identity's household is, and its
/// anchor. Signed by the recovery identity.
pub async fn recovery(
    State(s): State<AppState>,
    ip: ClientIp,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> R<Json<RecoveryView>> {
    s.limit("recovery", &ip.0)?;
    let Some((_, Some(row))) = authenticate(&s, &method, &uri, &headers, &body, None)? else {
        return Err(ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "sign this request",
        ));
    };
    if row.role != DeviceRole::Recovery {
        return Err(ApiErr::forbidden("only the recovery key can do this"));
    }
    let conn = s.db();
    let anchor: Vec<u8> = conn
        .query_row(
            "SELECT data FROM anchors WHERE space = ?1",
            [row.space.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(ApiErr::not_found)?;
    Ok(Json(RecoveryView {
        space: row.space,
        anchor,
    }))
}
