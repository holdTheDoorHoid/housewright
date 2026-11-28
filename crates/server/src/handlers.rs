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
use housewright_crypto::batch::{SealedBatch, verify_batch};
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
        return Err(ApiErr::conflict("that device is already registered"));
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
    s.limit("create", &ip.0)?;
    let req: CreateSpace = json(&body)?;
    if let Some(token) = &s.cfg().registration_token
        && req.registration_token.as_deref() != Some(token.as_str())
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
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    need_manager(&me)?;
    let req: UpdateRoster = json(&body)?;
    if req.roster.signer != me.id {
        return Err(ApiErr::forbidden(
            "a roster must be signed by the device sending it",
        ));
    }
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
            "UPDATE invitations SET state = 'accepted', offer = X'', request = NULL, reveal = NULL
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
            return Err(ApiErr::bad(
                "a batch uses a key epoch the household does not have",
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
            return Err(ApiErr::conflict(
                "this device already sent a different batch with that number",
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
                serde_json::to_string(b).map_err(|_| internal())?
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
    let space = space_id(&space)?;
    let me = device_of(&s, &method, &uri, &headers, &body, space)?;
    if me.role == DeviceRole::Recovery {
        return Err(ApiErr::forbidden("the recovery key does not read changes"));
    }
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let conn = s.db();
    let mut stmt = conn.prepare(
        "SELECT seq, received_ms, data FROM batches WHERE space = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![space.to_string(), q.after as i64, (limit + 1) as i64],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    )?;
    let mut batches = Vec::new();
    for r in rows {
        let (seq, received, data) = r?;
        let batch: SealedBatch = serde_json::from_str(&data).map_err(|_| internal())?;
        batches.push(Pulled {
            seq: seq as u64,
            received_ms: received as u64,
            batch,
        });
    }
    let more = batches.len() as u64 > limit;
    batches.truncate(limit as usize);
    Ok(Json(PullOps { batches, more }))
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
    let space = space_id(&space)?;
    device_of(&s, &method, &uri, &headers, &body, space)?;
    let req: BlobNames = json(&body)?;
    if req.names.len() > 2000 || !req.names.iter().all(|n| blob_name_ok(n)) {
        return Err(ApiErr::bad("bad file names"));
    }
    let conn = s.db();
    let mut names = Vec::new();
    for n in req.names {
        let have: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM blobs WHERE space = ?1 AND name = ?2)",
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
    let path = s.blob_path(space, &name);
    let dir = path.parent().expect("has a parent");
    let io = |e: std::io::Error| {
        eprintln!("housewright-server: file storage error: {}", e.kind());
        internal()
    };
    std::fs::create_dir_all(dir).map_err(io)?;
    if !path.exists() {
        let tmp = dir.join(format!(".{name}.tmp"));
        std::fs::write(&tmp, &body).map_err(io)?;
        std::fs::rename(&tmp, &path).map_err(io)?;
    }
    s.db().execute(
        "INSERT OR IGNORE INTO blobs (space, name, size, created_ms) VALUES (?1, ?2, ?3, ?4)",
        params![space.to_string(), name, body.len() as i64, s.now() as i64],
    )?;
    Ok(StatusCode::OK)
}

pub async fn get_blob(
    State(s): State<AppState>,
    Path((space, name)): Path<(String, String)>,
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
    match std::fs::read(s.blob_path(space, &name)) {
        Ok(b) => Ok(([("content-type", "application/octet-stream")], b).into_response()),
        Err(_) => Err(ApiErr::not_found()),
    }
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
    state: String,
}

fn load_invitation(conn: &Connection, id: &str, now: u64) -> R<InvRow> {
    if !invitation_id_ok(id) {
        return Err(ApiErr::not_found());
    }
    purge_invitations(conn, now)?;
    conn.query_row(
        "SELECT space, offer, request, reveal, state FROM invitations WHERE id = ?1",
        [id],
        |r| {
            Ok(InvRow {
                space: r.get(0)?,
                offer: r.get(1)?,
                request: r.get(2)?,
                reveal: r.get(3)?,
                state: r.get(4)?,
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
