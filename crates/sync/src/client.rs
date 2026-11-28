//! The sync engine: set up, sync, devices, removal, recovery.

use std::collections::BTreeSet;

use housewright_core::blobs::{BlobStore, referenced};
use housewright_core::ids::MemberId;
use housewright_core::model::{Attachment, Member, Role};
use housewright_core::oplog::{Op, RECORD_FIELD};
use housewright_core::store::{MAX_CLOCK_AHEAD_MS, Store};
use housewright_crypto::batch::{
    SealedBatch, ZERO_HASH, batch_hash, new_batch_id, open_batch, seal_batch, verify_batch,
};
use housewright_crypto::blob::{blob_name, open_blob, seal_blob};
use housewright_crypto::recovery::{Anchor, RecoveryKey};
use housewright_crypto::request::{H_SERVER_TIME, sign_request};
use housewright_crypto::roster::{
    DeviceRole, RemovedEntry, RosterBody, RosterEntry, SignedRoster, open_label, seal_label,
    verify_chain, verify_next,
};
use housewright_crypto::wire::*;
use housewright_crypto::wrap::{KeyEnvelope, unwrap, wrap};
use housewright_crypto::{DeviceId, DevicePublic, DeviceSecret, KeyChain, SpaceId};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;

use crate::state::{self, Head, Notice, Pending, State, SyncConfig, SyncStatus};
use crate::{HttpRequest, HttpResponse, Result, SyncError, Transport, check_server_url};
use housewright_crypto::CryptoError;

/// Operations per pushed batch, and a size cap for its plaintext (well
/// under the server's ciphertext cap, D-100).
const BATCH_OPS: usize = 400;
const BATCH_BYTES: usize = 512 * 1024;
/// One operation bigger than this cannot be sent at all.
const MAX_OP_BYTES: usize = 900 * 1024;
/// Candidate batches kept per number for a removed device.
const HELD_PER_NUMBER: u64 = 4;
/// Files larger than this are fetched when opened, not during sync.
const PREFETCH_MAX: u64 = 4 * 1024 * 1024;

/// What one sync did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    /// Batches that broke a device's chain or reused a number (forgery).
    pub tampered: usize,
    /// A removed device's batches kept until their chain is complete.
    pub held: usize,
    /// Authentic batches this version cannot read, kept for later.
    pub unreadable: usize,
    /// Batches too large to fetch, skipped.
    pub skipped_too_large: usize,
    pub pushed_ops: usize,
    /// Files that could not be sent or fetched this time (tried again).
    pub files_failed: usize,
    /// Batches of this device sent again to a server restored from backup.
    pub resent: usize,
    pub server_restored: bool,
    /// Missing batches fetched by number to close a gap.
    pub gaps_filled: usize,
    /// Batches whose roster claim could not be verified.
    pub bad_claims: usize,
    pub pulled_ops: usize,
    pub rejected_batches: usize,
    pub refused_future_ops: usize,
    pub files_up: usize,
    pub files_down: usize,
}

/// A device in the household, for the devices list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub member: String,
    pub member_name: String,
    pub role: DeviceRole,
    pub this_device: bool,
    pub added_ms: Option<u64>,
    pub last_seen_ms: Option<u64>,
}

/// The client. Holds the transport and anything that lives only in memory
/// (a pairing in progress, the measured clock offset).
pub struct SyncClient {
    transport: Box<dyn Transport>,
    /// Server time minus local time, learned from responses.
    offset_ms: i64,
    pub(crate) invite: Option<crate::pairing::InviteSession>,
    pub(crate) join: Option<crate::pairing::JoinSession>,
}

pub(crate) fn now_ms(store: &Store) -> u64 {
    store.now().as_millisecond().max(0) as u64
}

fn to_json<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).expect("protocol types serialise")
}

pub(crate) fn from_json<T: DeserializeOwned>(r: &HttpResponse) -> Result<T> {
    serde_json::from_slice(&r.body)
        .map_err(|e| SyncError::Invalid(format!("the server sent something unexpected: {e}")))
}

pub(crate) fn role_of(r: Role) -> DeviceRole {
    match r {
        Role::Owner => DeviceRole::Owner,
        Role::Member => DeviceRole::Member,
        Role::Limited => DeviceRole::Limited,
    }
}

impl SyncClient {
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            offset_ms: 0,
            invite: None,
            join: None,
        }
    }

    pub fn is_set_up(store: &Store) -> Result<bool> {
        state::is_set_up(store)
    }

    /// One HTTP exchange; signs it when `signer` is given. Retries once
    /// with a corrected clock if the server says ours is off.
    pub(crate) fn call(
        &mut self,
        store: &Store,
        server: &str,
        signer: Option<&DeviceSecret>,
        method: &str,
        path: &str,
        body: Vec<u8>,
    ) -> Result<HttpResponse> {
        for attempt in 0..2 {
            let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
            if let Some(s) = signer {
                let t = (now_ms(store) as i64).saturating_add(self.offset_ms).max(0) as u64;
                let auth = sign_request(s, method, path, t, &body)?;
                headers.extend(auth.headers().map(|(k, v)| (k.to_string(), v)));
            }
            let res = self
                .transport
                .send(HttpRequest {
                    method: method.to_string(),
                    url: format!("{server}{path}"),
                    headers,
                    body: body.clone(),
                })
                .map_err(SyncError::Offline)?;
            if let Some(t) = res
                .header(H_SERVER_TIME)
                .and_then(|v| v.parse::<u64>().ok())
            {
                self.offset_ms =
                    (t.min(i64::MAX as u64) as i64).saturating_sub(now_ms(store) as i64);
            }
            if (200..300).contains(&res.status) {
                return Ok(res);
            }
            let err = SyncError::from_response(&res);
            if let SyncError::Server { code, .. } = &err
                && code == "clock_skew"
                && attempt == 0
            {
                continue;
            }
            return Err(err);
        }
        unreachable!("the loop returns")
    }

    /// The measured clock difference, when it is worth telling anyone.
    fn clock_offset(&self) -> Option<i64> {
        (self.offset_ms.unsigned_abs() > 60_000).then_some(self.offset_ms)
    }

    // -- status -----------------------------------------------------------

    pub fn config(store: &Store) -> Result<Option<SyncConfig>> {
        match store.sync_get("config")? {
            Some(v) => Ok(Some(
                serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    pub fn status(&self, store: &Store) -> Result<SyncStatus> {
        let mut s = state::load_status(store)?;
        if s.clock_offset_ms.is_none() {
            s.clock_offset_ms = self.clock_offset();
        }
        Ok(s)
    }

    /// Local operations still waiting to be sent.
    pub fn pending_count(store: &Store) -> Result<u64> {
        let cursor: i64 = store
            .sync_get("acked_upto")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(store.unsynced_count(cursor)?)
    }

    /// "OK" on the device-change notices.
    pub fn dismiss_notices(store: &Store) -> Result<()> {
        let mut s = state::load_status(store)?;
        s.notices.clear();
        // The person has seen the warning about a fork: sending resumes
        // (D-106). A new fork sets it again.
        s.fork = false;
        state::save_status(store, &s)
    }

    pub fn confirm_recovery_saved(store: &Store) -> Result<()> {
        let mut s = state::load_status(store)?;
        s.recovery_confirmed = true;
        state::save_status(store, &s)
    }

    /// "Stop syncing on this device": forget the keys and settings here.
    /// The household's data stays on the device.
    pub fn forget(&mut self, store: &Store) -> Result<()> {
        self.invite = None;
        self.join = None;
        store.sync_forget()?;
        Ok(())
    }

    // -- setting up -------------------------------------------------------

    /// Check that an address is a Housewright server.
    pub fn health(&mut self, store: &Store, server: &str) -> Result<Health> {
        let server = check_server_url(server)?;
        let r = self.call(store, &server, None, "GET", "/v1/health", vec![])?;
        from_json(&r).map_err(|_| {
            SyncError::Invalid(
                "That address answered, but it is not a Housewright sync server.".into(),
            )
        })
    }

    /// Turn sync on for this household (the first device). Returns the
    /// recovery key, which is not stored anywhere and must be shown now.
    pub fn setup(
        &mut self,
        store: &mut Store,
        server: &str,
        device_name: &str,
        registration_token: Option<&str>,
    ) -> Result<Zeroizing<String>> {
        if state::is_set_up(store)? {
            return Err(SyncError::Invalid(
                "Sync is already set up on this device.".into(),
            ));
        }
        let server = check_server_url(server)?;
        let me_member = store.author()?;
        let member: Member = store.fetch(&me_member.to_string())?;
        if member.role != Role::Owner {
            return Err(SyncError::Invalid(
                "Only the household's owner can set up sync.".into(),
            ));
        }
        self.health(store, &server)?;
        let secret = DeviceSecret::generate()?;
        let recovery = RecoveryKey::generate()?;
        let rec = recovery.identity();
        let space = SpaceId::random()?;
        let chain = KeyChain::generate()?;
        let me = secret.public();
        let mut body = RosterBody {
            v: 1,
            space,
            version: 1,
            epoch: 0,
            prev: "0".repeat(64),
            genesis: String::new(),
            devices: vec![
                RosterEntry {
                    device: me.clone(),
                    member: me_member.to_string(),
                    role: DeviceRole::Owner,
                    added_in: 1,
                    label: seal_label(&chain, space, me.id, device_name)?,
                },
                RosterEntry {
                    device: rec.public(),
                    member: me_member.to_string(),
                    role: DeviceRole::Recovery,
                    added_in: 1,
                    label: seal_label(&chain, space, rec.id(), "Recovery key")?,
                },
            ],
            removed: vec![],
            made_ms: now_ms(store),
            history_batches: 0,
            history_head: String::new(),
        };
        // The household's history so far, sealed now as the founding
        // device's first batches; the genesis roster commits to how many
        // and to the last one's hash, so the founder's exception to "only
        // in your own name" covers exactly these (D-110).
        let mut history: Vec<Pending> = Vec::new();
        let mut prev = ZERO_HASH.to_string();
        let mut cursor = 0i64;
        let mut unsent = 0u64;
        loop {
            match seal_next(
                store,
                &secret,
                &chain,
                space,
                None,
                history.len() as u64 + 1,
                &prev,
                cursor,
                &mut unsent,
            )? {
                Sealed::Done => break,
                Sealed::Skipped(upto) => cursor = upto,
                Sealed::Batch(batch, upto, _) => {
                    prev = batch_hash(&batch)?;
                    cursor = upto;
                    history.push(Pending {
                        batch: *batch,
                        upto,
                        replaces: None,
                    });
                }
            }
        }
        body.history_batches = history.len() as u64;
        if !history.is_empty() {
            body.history_head = prev.clone();
        }
        body.sort();
        let roster = SignedRoster::sign(&body, &secret)?;
        let envelopes = vec![
            wrap(&secret, space, &me, &chain)?,
            wrap(&secret, space, &rec.public(), &chain)?,
        ];
        let anchor = recovery.seal_anchor(
            space,
            &Anchor {
                genesis: roster.hash(),
            },
        )?;
        let req = CreateSpace {
            space,
            roster: roster.clone(),
            envelopes,
            anchor,
            registration_token: registration_token
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        };
        self.call(
            store,
            &server,
            Some(&secret),
            "POST",
            "/v1/spaces",
            to_json(&req),
        )?;
        let st = State {
            config: SyncConfig {
                server,
                space,
                device: me.id,
                member: me_member.to_string(),
                role: DeviceRole::Owner,
            },
            secret,
            chain,
            rosters: vec![roster],
            bodies: vec![body],
            cursor: 0,
            push_cursor: cursor,
            next_device_seq: history.len() as u64 + 1,
            acked_seq: 0,
            acked_upto: 0,
            heads: Default::default(),
            own_head: prev,
            status: SyncStatus {
                unsent_too_large: unsent,
                ..Default::default()
            },
        };
        for (i, p) in history.iter().enumerate() {
            save_sent(store, i as u64 + 1, p)?;
        }
        st.save(store)?;
        Ok(recovery.display())
    }

    // -- syncing ----------------------------------------------------------

    /// Push, pull and move files. Safe to call at any time, as often as
    /// wanted; safe to interrupt.
    pub fn sync(&mut self, store: &mut Store, blobs: &mut dyn BlobStore) -> Result<SyncReport> {
        let mut st = State::load(store)?;
        let out = self.sync_inner(store, blobs, &mut st);
        st.status.clock_offset_ms = self.clock_offset();
        match &out {
            Ok(r) => {
                st.status.last_sync_ms = Some(now_ms(store));
                st.status.last_error = None;
                st.status.last_error_code = None;
                st.status.clock_ahead = false;
                st.status.rejected_batches += r.rejected_batches as u64;
                st.status.refused_future_ops += r.refused_future_ops as u64;
            }
            Err(e) => {
                st.status.last_error = Some(e.plain());
                st.status.last_error_code = Some(e.code().to_string());
                st.status.clock_ahead = matches!(e, SyncError::ClockAhead);
                if matches!(e, SyncError::Removed) {
                    st.status.removed = true;
                }
            }
        }
        st.save(store)?;
        out
    }

    fn sync_inner(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        st: &mut State,
    ) -> Result<SyncReport> {
        if st.status.removed {
            return Err(SyncError::Removed);
        }
        let mut report = SyncReport::default();
        self.refresh(store, st)?;
        // A wrong clock does not stop sync: requests are signed with the
        // corrected time, pulling is safe, and the server holds back
        // changes stamped in the future (ClockAhead). The interface warns
        // from `clock_offset_ms`.
        // Pull first, so a long-offline device has the latest roster
        // and keys before sealing anything; then push.
        self.pull(store, st, &mut report)?;
        if store.meta("household_id")?.is_none() {
            let member: MemberId = st
                .config
                .member
                .parse()
                .map_err(|_| SyncError::Invalid("bad member id".into()))?;
            store.adopt_household(member)?;
        }
        if st.config.role != DeviceRole::Recovery {
            self.push(store, st, &mut report)?;
            self.sync_files(store, blobs, st, &mut report)?;
        }
        Ok(report)
    }

    /// Fetch newer roster versions and keys; check them all.
    pub(crate) fn refresh(&mut self, store: &Store, st: &mut State) -> Result<()> {
        let path = format!(
            "/v1/s/{}/roster?after={}",
            st.config.space,
            st.latest().version
        );
        let server = st.config.server.clone();
        let r = self.call(store, &server, Some(&st.secret), "GET", &path, vec![])?;
        let list: RosterList = from_json(&r)?;
        for next in list.rosters {
            let prev_signed = st.rosters.last().expect("non-empty");
            let body = verify_next(prev_signed, st.latest(), &next)?;
            if next.signer != st.config.device {
                for text in describe_change(st, st.latest(), &body, &next) {
                    st.status.notices.push(Notice {
                        at_ms: body.made_ms,
                        text,
                    });
                }
                let n = st.status.notices.len();
                if n > 20 {
                    st.status.notices.drain(..n - 20);
                }
            }
            st.rosters.push(next);
            st.bodies.push(body);
        }
        if st.latest().find(st.config.device).is_none() {
            st.status.removed = true;
            return Err(SyncError::Removed);
        }
        if st.latest().epoch > st.chain.current_epoch() {
            self.fetch_keys(store, st)?;
        }
        Ok(())
    }

    /// Open the newest key envelope for this device.
    pub(crate) fn fetch_keys(&mut self, store: &Store, st: &mut State) -> Result<()> {
        let path = format!("/v1/s/{}/keys", st.config.space);
        let server = st.config.server.clone();
        let r = self.call(store, &server, Some(&st.secret), "GET", &path, vec![])?;
        let list: KeyList = from_json(&r)?;
        let want = st.latest().epoch;
        let env = list
            .envelopes
            .iter()
            .filter(|e| e.space == st.config.space)
            .max_by_key(|e| e.epoch)
            .ok_or_else(|| SyncError::Invalid("no keys for this device yet".into()))?;
        let chain = open_envelope(&st.secret, &st.bodies, env)?;
        if !st.chain.is_prefix_of(&chain) {
            return Err(SyncError::Invalid(
                "the new household keys do not extend the old ones".into(),
            ));
        }
        if chain.current_epoch() < want {
            return Err(SyncError::Invalid(
                "the household's newest keys have not arrived yet".into(),
            ));
        }
        st.chain = chain;
        Ok(())
    }

    fn push(&mut self, store: &mut Store, st: &mut State, report: &mut SyncReport) -> Result<()> {
        // A verified newer roster that the server will not give this
        // device, or a verified fork: seal nothing new until resolved
        // (D-103, D-106). Unverifiable claims never get here.
        if st.status.fork || st.status.newest_roster_seen > st.latest().version {
            return Err(SyncError::RosterHidden);
        }
        let path = format!("/v1/s/{}/ops", st.config.space);
        let server = st.config.server.clone();
        // Does the server still hold everything this device sent? After a
        // restore from backup, or if it lost a batch, send those again
        // from the copies kept here (D-107).
        if st.acked_seq > 0 {
            let hp = format!("/v1/s/{}/chain/{}", st.config.space, st.config.device);
            let head: ChainHead =
                from_json(&self.call(store, &server, Some(&st.secret), "GET", &hp, vec![])?)?;
            for n in head.missing {
                if n > st.acked_seq {
                    break;
                }
                let p = load_sent(store, n)?;
                let body = to_json(&PushOps {
                    batches: vec![p.batch],
                });
                self.call(store, &server, Some(&st.secret), "POST", &path, body)?;
                report.resent += 1;
            }
            if head.seq < st.acked_seq {
                report.resent += (st.acked_seq - head.seq) as usize;
                st.acked_seq = head.seq;
            }
        }
        loop {
            // Send every sealed batch the server has not acknowledged, in
            // order (after a server restore, that can be many; D-107).
            while st.acked_seq + 1 < st.next_device_seq {
                let seq = st.acked_seq + 1;
                let mut p = load_sent(store, seq)?;
                // The last one, sealed before this device learned of a newer
                // epoch: seal it again under the new key, same number and
                // link (L4).
                if seq + 1 == st.next_device_seq
                    && p.replaces.is_none()
                    && p.batch.header.epoch < st.chain.current_epoch()
                {
                    let me = st.secret.public();
                    let plain =
                        Zeroizing::new(open_batch(&p.batch, st.config.space, &st.chain, &me)?);
                    let fresh = seal_batch(
                        &st.secret,
                        &st.chain,
                        st.config.space,
                        new_batch_id()?,
                        p.batch.header.device_seq,
                        &p.batch.header.prev,
                        p.batch.header.max_wall_ms,
                        &plain,
                    )?;
                    st.own_head = batch_hash(&fresh)?;
                    p = Pending {
                        replaces: Some(p.batch),
                        batch: fresh,
                        upto: p.upto,
                    };
                    save_sent(store, seq, &p)?;
                    st.save(store)?;
                }
                let body = to_json(&PushOps {
                    batches: vec![p.batch.clone()],
                });
                match self.call(store, &server, Some(&st.secret), "POST", &path, body) {
                    Ok(_) => {}
                    // The server already has the original of a re-sealed
                    // batch: that one counts.
                    Err(SyncError::Server { code, .. })
                        if code == "seq_reused" && p.replaces.is_some() =>
                    {
                        let orig = p.replaces.take().expect("checked");
                        st.own_head = batch_hash(&orig)?;
                        save_sent(
                            store,
                            seq,
                            &Pending {
                                batch: orig,
                                upto: p.upto,
                                replaces: None,
                            },
                        )?;
                    }
                    // The server lacks this device's earlier batches (it was
                    // restored from a backup): send them again from here.
                    Err(SyncError::Server { code, .. }) if code == "chain" => {
                        let path = format!("/v1/s/{}/chain/{}", st.config.space, st.config.device);
                        let head: ChainHead = from_json(&self.call(
                            store,
                            &server,
                            Some(&st.secret),
                            "GET",
                            &path,
                            vec![],
                        )?)?;
                        if head.seq + 1 >= seq {
                            return Err(SyncError::Invalid(
                                "The server refused this device's changes; they are kept here."
                                    .into(),
                            ));
                        }
                        st.acked_seq = head.seq;
                        report.resent += (seq - head.seq - 1) as usize;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
                st.acked_seq = seq;
                st.acked_upto = p.upto;
                st.save(store)?;
            }
            let roster = st.rosters.last().cloned();
            match seal_next(
                store,
                &st.secret,
                &st.chain,
                st.config.space,
                roster,
                st.next_device_seq,
                &st.own_head,
                st.push_cursor,
                &mut st.status.unsent_too_large,
            )? {
                Sealed::Done => return Ok(()),
                Sealed::Skipped(upto) => {
                    st.push_cursor = upto;
                    st.save(store)?;
                }
                Sealed::Batch(batch, upto, n) => {
                    report.pushed_ops += n;
                    st.own_head = batch_hash(&batch)?;
                    // Saved before sending: a retry sends these same bytes.
                    save_sent(
                        store,
                        st.next_device_seq,
                        &Pending {
                            batch: *batch,
                            upto,
                            replaces: None,
                        },
                    )?;
                    st.next_device_seq += 1;
                    st.push_cursor = upto;
                    st.save(store)?;
                }
            }
        }
    }

    fn pull(&mut self, store: &mut Store, st: &mut State, report: &mut SyncReport) -> Result<()> {
        let server = st.config.server.clone();
        let mut limit = 200u32;
        loop {
            let path = format!(
                "/v1/s/{}/ops?after={}&limit={limit}",
                st.config.space, st.cursor
            );
            // A page that cannot be read (too big for this device, cut
            // off): ask for fewer, down to one (D-100).
            let page: PullOps =
                match self.call(store, &server, Some(&st.secret), "GET", &path, vec![]) {
                    Ok(r) => match from_json(&r) {
                        Ok(p) => p,
                        Err(_) if limit > 1 => {
                            limit /= 2;
                            continue;
                        }
                        Err(e) => return Err(e),
                    },
                    Err(SyncError::Offline(_)) if limit > 1 => {
                        limit /= 2;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
            // A server restored from a backup: its numbers start again
            // below ours. Read everything again (duplicates are ignored),
            // and give it back what it lost (D-107).
            if page.head < st.cursor {
                st.cursor = 0;
                report.server_restored = true;
                continue;
            }
            if page.roster_version > 0 && page.roster_version < st.latest().version {
                self.restore_rosters(store, st, page.roster_version)?;
                report.server_restored = true;
            }
            for p in &page.batches {
                match &p.batch {
                    None => {
                        report.skipped_too_large += 1;
                        st.status.skipped_too_large += 1;
                        // A gap in that device's chain: try to fetch it on
                        // its own; if that fails too it is reported missing.
                        if let Some(dev) = p.device
                            && dev != st.config.device
                        {
                            self.fill_gap(store, st, dev, report)?;
                        }
                    }
                    Some(b) if b.header.device == st.config.device => {}
                    Some(b) => self.receive(store, st, p, b, report)?,
                }
                st.cursor = p.seq;
            }
            st.save(store)?;
            if !page.more {
                break;
            }
        }
        self.retry_unreadable(store, st, report)
    }

    /// An owner's device gives a restored server the roster versions it
    /// lost, with fresh key envelopes (D-107).
    fn restore_rosters(
        &mut self,
        store: &Store,
        st: &mut State,
        server_version: u64,
    ) -> Result<()> {
        if st.config.role != DeviceRole::Owner {
            return Ok(());
        }
        let server = st.config.server.clone();
        for v in server_version + 1..=st.latest().version {
            let i = (v - 1) as usize;
            let (signed, body, prev) = (&st.rosters[i], &st.bodies[i], &st.bodies[i - 1]);
            if prev.recovery().map(|r| r.device.id) != body.recovery().map(|r| r.device.id) {
                // A new recovery key's anchor needs the paper.
                return Err(SyncError::Invalid(
                    "The sync server lost a recent change of recovery key. Make a new recovery key to repair it.".into(),
                ));
            }
            let chain = st
                .chain
                .truncated(body.epoch)
                .ok_or_else(|| SyncError::Invalid("missing keys".into()))?;
            let rotated = body.epoch != prev.epoch;
            let mut envelopes = Vec::new();
            for e in &body.devices {
                if rotated || prev.find(e.device.id).is_none() {
                    envelopes.push(wrap(&st.secret, body.space, &e.device, &chain)?);
                }
            }
            let req = UpdateRoster {
                roster: signed.clone(),
                envelopes,
                anchor: None,
                invitation: None,
            };
            let path = format!("/v1/s/{}/roster", body.space);
            self.call(
                store,
                &server,
                Some(&st.secret),
                "POST",
                &path,
                to_json(&req),
            )?;
        }
        Ok(())
    }

    /// One pulled batch from another device: the chain rules (D-99,
    /// D-107), then the content checks, then merge.
    fn receive(
        &mut self,
        store: &mut Store,
        st: &mut State,
        p: &Pulled,
        b: &SealedBatch,
        report: &mut SyncReport,
    ) -> Result<()> {
        let dev = b.header.device;
        let mut who = author_of(st.latest(), dev);
        if matches!(who, Author::Unknown) {
            // Perhaps added since we last looked.
            self.refresh(store, st)?;
            who = author_of(st.latest(), dev);
        }
        let head = st.heads.get(&dev).cloned().unwrap_or_else(Head::start);
        let seq = b.header.device_seq;
        let hash = batch_hash(b)?;
        if seq <= head.seq {
            if !(seq == head.seq && hash == head.hash)
                && !self.is_known(store, st, dev, seq, &hash)?
            {
                // A second, different batch under a number already used:
                // forged or forked. Refused everywhere, not only by the
                // server.
                report.rejected_batches += 1;
                report.tampered += 1;
            }
            return Ok(());
        }
        match who {
            Author::Unknown => report.rejected_batches += 1,
            Author::Current(entry) => {
                // Only the device's own signature gets a batch held.
                if verify_batch(b, &entry.device).is_err() {
                    report.rejected_batches += 1;
                    report.tampered += 1;
                    return Ok(());
                }
                if seq == head.seq + 1 {
                    if b.header.prev != head.hash {
                        report.rejected_batches += 1;
                        report.tampered += 1;
                        return Ok(());
                    }
                    if self.consume(store, st, p, b, &entry, report)? {
                        st.heads.insert(dev, Head { seq, hash });
                        self.drain_gap(store, st, dev, &entry, report)?;
                    }
                } else {
                    // A gap: hold this one (one per number) and fetch the
                    // missing ones by number (N2).
                    if !hold(store, "gap", dev, seq, p, &hash, report)? {
                        return Ok(());
                    }
                    self.fill_gap(store, st, dev, report)?;
                }
            }
            Author::Removed(r) => {
                // A removed device: only batches on the chain that ends at
                // the hash recorded when it was removed, signed by it, at
                // most one per number and no more than it can have sent
                // (N5). Applied once that whole chain is here.
                if seq > r.last_seq || verify_batch(b, &r.entry.device).is_err() {
                    report.rejected_batches += 1;
                    return Ok(());
                }
                // Several candidates per number (the removed device can sign
                // forks of its own chain); only the chain ending at the
                // recorded hash is ever applied, so a fork shown first
                // cannot block the real batches.
                let key = format!("held:{dev}:{seq:020}:{hash}");
                let head_seq = st.heads.get(&dev).map(|h| h.seq).unwrap_or(0);
                let room = (r.last_seq - head_seq) * HELD_PER_NUMBER;
                if store.sync_get(&key)?.is_some() {
                    return Ok(());
                }
                if store.sync_keys(&format!("held:{dev}:{seq:020}:"))?.len() as u64
                    >= HELD_PER_NUMBER
                    || store.sync_keys(&format!("held:{dev}:"))?.len() as u64 >= room
                {
                    report.rejected_batches += 1;
                    report.tampered += 1;
                    return Ok(());
                }
                store.sync_set(
                    &key,
                    &serde_json::to_string(p).map_err(|e| SyncError::Invalid(e.to_string()))?,
                )?;
                report.held += 1;
                self.resolve_held(store, st, &r, report)?;
            }
        }
        Ok(())
    }

    /// Whether `hash` is the batch already applied at `seq` (a batch seen
    /// again after a cursor reset is not tampering).
    fn is_known(
        &self,
        store: &Store,
        _st: &State,
        dev: DeviceId,
        seq: u64,
        hash: &str,
    ) -> Result<bool> {
        Ok(store.sync_get(&format!("seen:{dev}:{seq:020}"))?.as_deref() == Some(hash))
    }

    /// Fetch the missing numbers of a device's chain one by one, then apply
    /// what was held. What cannot be fetched is reported, plainly.
    fn fill_gap(
        &mut self,
        store: &mut Store,
        st: &mut State,
        dev: DeviceId,
        report: &mut SyncReport,
    ) -> Result<()> {
        let Author::Current(entry) = author_of(st.latest(), dev) else {
            return Ok(());
        };
        let server = st.config.server.clone();
        let prefix = format!("gap:{dev}:");
        loop {
            let head = st.heads.get(&dev).cloned().unwrap_or_else(Head::start);
            let next = head.seq + 1;
            if store.sync_get(&format!("{prefix}{next:020}"))?.is_some() {
                self.drain_gap(store, st, dev, &entry, report)?;
                continue;
            }
            if store.sync_keys(&prefix)?.is_empty() {
                let name = device_name(st, dev);
                st.status.missing_from.retain(|n| *n != name);
                return Ok(());
            }
            let path = format!("/v1/s/{}/chain/{dev}/{next}", st.config.space);
            let one: OneBatch =
                match self.call(store, &server, Some(&st.secret), "GET", &path, vec![]) {
                    Ok(r) => from_json(&r)?,
                    Err(SyncError::Server {
                        status: 404 | 413, ..
                    }) => {
                        let name = device_name(st, dev);
                        if !st.status.missing_from.contains(&name) {
                            st.status.missing_from.push(name);
                        }
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                };
            let Some(b) = one.batch.batch.clone() else {
                return Ok(());
            };
            let hash = batch_hash(&b)?;
            if b.header.device != dev
                || b.header.device_seq != next
                || b.header.prev != head.hash
                || verify_batch(&b, &entry.device).is_err()
            {
                report.tampered += 1;
                return Ok(());
            }
            if !self.consume(store, st, &one.batch, &b, &entry, report)? {
                return Ok(());
            }
            st.heads.insert(dev, Head { seq: next, hash });
            report.gaps_filled += 1;
        }
    }

    /// Apply held batches that now follow the device's head.
    fn drain_gap(
        &mut self,
        store: &mut Store,
        st: &mut State,
        dev: DeviceId,
        entry: &RosterEntry,
        report: &mut SyncReport,
    ) -> Result<()> {
        loop {
            let head = st.heads.get(&dev).cloned().unwrap_or_else(Head::start);
            let key = format!("gap:{dev}:{:020}", head.seq + 1);
            let Some(v) = store.sync_get(&key)? else {
                if store.sync_keys(&format!("gap:{dev}:"))?.is_empty() {
                    let name = device_name(st, dev);
                    st.status.missing_from.retain(|n| *n != name);
                }
                return Ok(());
            };
            store.sync_remove(&key)?;
            let p: Pulled =
                serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))?;
            let Some(b) = p.batch.clone() else { continue };
            if b.header.prev != head.hash {
                report.tampered += 1;
                continue;
            }
            if self.consume(store, st, &p, &b, entry, report)? {
                st.heads.insert(
                    dev,
                    Head {
                        seq: b.header.device_seq,
                        hash: batch_hash(&b)?,
                    },
                );
            }
        }
    }

    /// Apply a removed device's held batches once the chain from our head
    /// to its recorded last hash is complete.
    fn resolve_held(
        &mut self,
        store: &mut Store,
        st: &mut State,
        r: &RemovedEntry,
        report: &mut SyncReport,
    ) -> Result<()> {
        let dev = r.entry.device.id;
        let prefix = format!("held:{dev}:");
        let head = st.heads.get(&dev).cloned().unwrap_or_else(Head::start);
        if head.seq >= r.last_seq {
            for k in store.sync_keys(&prefix)? {
                store.sync_remove(&k)?;
            }
            return Ok(());
        }
        // Walk back from the recorded last hash to our head.
        let mut chain: Vec<(Pulled, SealedBatch)> = Vec::new();
        let mut want = r.last_hash.clone();
        for n in (head.seq + 1..=r.last_seq).rev() {
            let Some(v) = store.sync_get(&format!("{prefix}{n:020}:{want}"))? else {
                return Ok(()); // not complete yet
            };
            let p: Pulled =
                serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))?;
            let b = p
                .batch
                .clone()
                .ok_or_else(|| SyncError::Invalid("held batch".into()))?;
            want = b.header.prev.clone();
            chain.push((p, b));
        }
        if want != head.hash {
            report.tampered += 1;
            return Ok(());
        }
        chain.reverse();
        for (p, b) in &chain {
            if !self.consume(store, st, p, b, &r.entry, report)? {
                break;
            }
            st.heads.insert(
                dev,
                Head {
                    seq: b.header.device_seq,
                    hash: batch_hash(b)?,
                },
            );
        }
        for k in store.sync_keys(&prefix)? {
            store.sync_remove(&k)?;
        }
        Ok(())
    }

    /// Open, check and merge one batch that passed the chain rules.
    /// Returns false if it was not authentic (the chain does not move).
    fn consume(
        &mut self,
        store: &mut Store,
        st: &mut State,
        p: &Pulled,
        b: &SealedBatch,
        author: &RosterEntry,
        report: &mut SyncReport,
    ) -> Result<bool> {
        match self.open_checked(store, st, p, b, author, report)? {
            Opened::Ops(ops) => {
                let a = store.apply_synced_ops(&ops, p.received_ms)?;
                report.pulled_ops += a.applied;
                report.refused_future_ops += a.too_far_ahead;
                report.tampered += a.conflicts;
            }
            Opened::Unreadable => {
                // Probably written by a newer version of the app: kept and
                // tried again after an update, never dropped (L3b).
                let v = serde_json::to_string(&(p, author))
                    .map_err(|e| SyncError::Invalid(e.to_string()))?;
                store.sync_set(&format!("unread:{:020}", p.seq), &v)?;
                report.unreadable += 1;
            }
            Opened::Refused => report.rejected_batches += 1,
            Opened::Forged => {
                report.rejected_batches += 1;
                report.tampered += 1;
                return Ok(false);
            }
        }
        store.sync_set(
            &format!("seen:{}:{:020}", b.header.device, b.header.device_seq),
            &batch_hash(b)?,
        )?;
        Ok(true)
    }

    fn retry_unreadable(
        &mut self,
        store: &mut Store,
        st: &mut State,
        report: &mut SyncReport,
    ) -> Result<()> {
        for k in store.sync_keys("unread:")? {
            let Some(v) = store.sync_get(&k)? else {
                continue;
            };
            let (p, author): (Pulled, RosterEntry) =
                serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))?;
            let Some(b) = p.batch.clone() else {
                store.sync_remove(&k)?;
                continue;
            };
            match self.open_checked(store, st, &p, &b, &author, report)? {
                Opened::Unreadable => {}
                Opened::Ops(ops) => {
                    let a = store.apply_synced_ops(&ops, p.received_ms)?;
                    report.pulled_ops += a.applied;
                    store.sync_remove(&k)?;
                }
                Opened::Refused | Opened::Forged => store.sync_remove(&k)?,
            }
        }
        Ok(())
    }

    /// Check a batch's roster claim (D-106). Only a version signed by one
    /// of the household's owner devices counts; anything else is the
    /// author's tampering, counted and otherwise ignored.
    fn check_claim(&mut self, store: &Store, st: &mut State, claim: &SignedRoster) -> bool {
        let Ok(body) = claim.parse() else {
            return false;
        };
        if body.space != st.config.space || body.version == 0 {
            return false;
        }
        let genesis = st.rosters[0].hash();
        if body.version == 1 {
            return claim.hash() == genesis;
        }
        if body.genesis != genesis {
            return false;
        }
        if body.version > st.latest().version {
            let _ = self.refresh(store, st);
        }
        if body.version <= st.latest().version {
            let ours = &st.rosters[(body.version - 1) as usize];
            if ours.hash() == claim.hash() {
                return true;
            }
            // A different version with the same number: real only if an
            // owner of the version before signed it; then it is a fork.
            let prev = &st.bodies[(body.version - 2) as usize];
            let genuine = prev
                .find(claim.signer)
                .filter(|e| e.role.can_manage())
                .is_some_and(|e| claim.check_sig(&e.device).is_ok());
            if genuine {
                st.status.fork = true;
            }
            return genuine;
        }
        // Newer than anything the server gives us: credible only if one of
        // our current owner devices signed it.
        let genuine = st
            .latest()
            .find(claim.signer)
            .filter(|e| e.role.can_manage())
            .is_some_and(|e| claim.check_sig(&e.device).is_ok());
        if genuine {
            st.status.newest_roster_seen = st.status.newest_roster_seen.max(body.version);
        }
        genuine
    }

    /// Every content check a batch must pass. Uses only data every device
    /// sees the same way, so every device decides the same.
    fn open_checked(
        &mut self,
        store: &Store,
        st: &mut State,
        p: &Pulled,
        b: &SealedBatch,
        author: &RosterEntry,
        report: &mut SyncReport,
    ) -> Result<Opened> {
        let h = &b.header;
        if author.role == DeviceRole::Recovery {
            return Ok(Opened::Refused);
        }
        if h.epoch > st.chain.current_epoch() {
            self.refresh(store, st)?;
            self.fetch_keys(store, st)?;
        }
        let Ok(plain) = open_batch(b, st.config.space, &st.chain, &author.device) else {
            return Ok(Opened::Forged);
        };
        let plain = Zeroizing::new(plain);
        let Ok(body) = serde_json::from_slice::<BatchBody>(&plain) else {
            return Ok(Opened::Unreadable);
        };
        let Ok(ops) = serde_json::from_value::<Vec<Op>>(body.ops) else {
            return Ok(Opened::Unreadable);
        };
        if let Some(claim) = &body.roster
            && !self.check_claim(store, st, claim)
        {
            report.bad_claims += 1;
            st.status.bad_claims += 1;
        }
        // The clock guard, at batch level: the claimed maximum must be
        // honest and not ahead of when the server received the batch.
        if h.max_wall_ms > p.received_ms.saturating_add(MAX_CLOCK_AHEAD_MS)
            || ops.iter().any(|o| o.hlc.wall_ms > h.max_wall_ms)
        {
            return Ok(Opened::Refused);
        }
        // Authorship (D-104): every operation, and the `by` of every
        // record, names the member the device belongs to. The one exception
        // is the founding device's history batches, numbered and hashed in
        // the genesis roster (D-110), so it cannot be used later.
        let genesis = &st.bodies[0];
        let history =
            st.rosters[0].signer == author.device.id && h.device_seq <= genesis.history_batches;
        if history
            && h.device_seq == genesis.history_batches
            && batch_hash(b)? != genesis.history_head
        {
            return Ok(Opened::Refused);
        }
        if !history {
            for o in &ops {
                if o.author.to_string() != author.member {
                    return Ok(Opened::Refused);
                }
                if o.field == RECORD_FIELD
                    && let Some(by) = o.value.get("by")
                    && !by.is_null()
                    && by.as_str() != Some(author.member.as_str())
                {
                    return Ok(Opened::Refused);
                }
            }
        }
        Ok(Opened::Ops(ops))
    }

    fn sync_files(
        &mut self,
        store: &Store,
        blobs: &mut dyn BlobStore,
        st: &mut State,
        report: &mut SyncReport,
    ) -> Result<()> {
        let wanted = referenced(store)?;
        if wanted.is_empty() {
            return Ok(());
        }
        let server = st.config.server.clone();
        let space = st.config.space;
        // Upload what we have and the server lacks.
        let local: Vec<String> = wanted
            .iter()
            .filter(|h| blobs.has(h).unwrap_or(false))
            .cloned()
            .collect();
        for chunk in local.chunks(500) {
            let names: Vec<String> = chunk
                .iter()
                .map(|h| blob_name(&st.chain, space, h))
                .collect::<std::result::Result<_, _>>()?;
            let path = format!("/v1/s/{space}/blobs/missing");
            let r = self.call(
                store,
                &server,
                Some(&st.secret),
                "POST",
                &path,
                to_json(&BlobNames {
                    names: names.clone(),
                }),
            )?;
            let missing: BTreeSet<String> = from_json::<BlobNames>(&r)?.names.into_iter().collect();
            for (hash, name) in chunk.iter().zip(&names) {
                if missing.contains(name)
                    && let Some(bytes) = blobs.get(hash)?
                {
                    let sealed = seal_blob(&st.chain, space, name, &bytes)?;
                    let path = format!("/v1/s/{space}/blobs/{name}");
                    // One file failing (too big, out of space) does not stop
                    // the others (L2).
                    match self.call(store, &server, Some(&st.secret), "PUT", &path, sealed) {
                        Ok(_) => report.files_up += 1,
                        Err(e @ SyncError::Offline(_)) => return Err(e),
                        Err(_) => report.files_failed += 1,
                    }
                }
            }
        }
        // Download thumbnails and smaller files we lack; big ones wait
        // until someone opens them.
        let mut small = BTreeSet::new();
        for a in store.list::<Attachment>()? {
            if let Some(t) = a.thumb_sha256 {
                small.insert(t);
            }
            if a.size <= PREFETCH_MAX {
                small.insert(a.sha256);
            }
        }
        for hash in wanted.iter().filter(|h| small.contains(*h)) {
            if blobs.has(hash)? {
                continue;
            }
            let mut got = self.download(store, blobs, st, hash, false);
            if matches!(got, Err(SyncError::StaleKeys)) {
                self.refresh(store, st)?;
                if st.latest().epoch > st.chain.current_epoch() {
                    self.fetch_keys(store, st)?;
                }
                got = self.download(store, blobs, st, hash, true);
            }
            match got {
                Ok(Some(_)) => report.files_down += 1,
                Ok(None) => {}
                Err(e @ SyncError::Offline(_)) => return Err(e),
                Err(_) => report.files_failed += 1,
            }
        }
        Ok(())
    }

    /// `refreshed`: keys were just fetched, so a missing key is no longer
    /// a reason to wait.
    fn download(
        &mut self,
        store: &Store,
        blobs: &mut dyn BlobStore,
        st: &State,
        hash: &str,
        refreshed: bool,
    ) -> Result<Option<Vec<u8>>> {
        let name = blob_name(&st.chain, st.config.space, hash)?;
        let path = format!("/v1/s/{}/blobs/{name}", st.config.space);
        let server = st.config.server.clone();
        let r = match self.call(store, &server, Some(&st.secret), "GET", &path, vec![]) {
            Ok(r) => r,
            // Not uploaded yet by the device that has it.
            Err(SyncError::Server { status: 404, .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let bytes = match open_blob(&st.chain, st.config.space, &name, &r.body, hash) {
            Ok(b) => b,
            // Sealed under a key this device has not got yet: never a
            // reason to doubt the file (N3). The caller refreshes keys.
            // (An epoch beyond the household's own is not stale keys: the
            // file is bad.)
            Err(CryptoError::Rejected(m))
                if m.starts_with("no key for epoch")
                    && (!refreshed
                        || r.body
                            .get(4..8)
                            .map(|e| u32::from_be_bytes(e.try_into().expect("4 bytes")))
                            <= Some(st.latest().epoch)) =>
            {
                return Err(SyncError::StaleKeys);
            }
            Err(e) => {
                // Decrypted with the right key and still wrong (damaged, or
                // garbage uploaded under this name first). Nothing is
                // deleted: the server marks it, so a device holding the
                // real file sends it again and that copy replaces it (D-108).
                let _ = self.call(
                    store,
                    &server,
                    Some(&st.secret),
                    "POST",
                    &format!("{path}/bad"),
                    vec![],
                );
                return Err(e.into());
            }
        };
        let got = blobs.put(&bytes)?;
        debug_assert_eq!(got, hash);
        Ok(Some(bytes))
    }

    /// A file that is not on this device yet: fetch it now (when it is
    /// opened). `Ok(None)` if sync is off or nobody has uploaded it.
    pub fn fetch_file(
        &mut self,
        store: &Store,
        blobs: &mut dyn BlobStore,
        hash: &str,
    ) -> Result<Option<Vec<u8>>> {
        if !state::is_set_up(store)? {
            return Ok(None);
        }
        let mut st = State::load(store)?;
        if st.status.removed {
            return Ok(None);
        }
        match self.download(store, blobs, &st, hash, false) {
            Err(SyncError::StaleKeys) => {
                self.refresh(store, &mut st)?;
                if st.latest().epoch > st.chain.current_epoch() {
                    self.fetch_keys(store, &mut st)?;
                }
                st.save(store)?;
                self.download(store, blobs, &st, hash, true)
            }
            r => r,
        }
    }

    // -- devices ----------------------------------------------------------

    /// The household's devices, from the verified roster, with "last seen"
    /// from the server when it can be reached.
    pub fn devices(&mut self, store: &Store) -> Result<Vec<DeviceInfo>> {
        let mut st = State::load(store)?;
        let online = self.refresh(store, &mut st).is_ok();
        st.save(store)?;
        let mut seen = Vec::new();
        if online {
            let path = format!("/v1/s/{}/devices", st.config.space);
            let server = st.config.server.clone();
            if let Ok(r) = self.call(store, &server, Some(&st.secret), "GET", &path, vec![]) {
                seen = from_json::<DeviceList>(&r)?.devices;
            }
        }
        let latest = st.latest();
        let mut out = Vec::new();
        for e in &latest.devices {
            if e.role == DeviceRole::Recovery {
                continue;
            }
            let s = seen.iter().find(|d| d.id == e.device.id);
            let member_name =
                store.member_name(e.member.parse().unwrap_or(MemberId(Default::default())));
            out.push(DeviceInfo {
                id: e.device.id.to_string(),
                name: open_label(&st.chain, latest.space, e.device.id, &e.label)
                    .unwrap_or_else(|_| "A device".into()),
                member: e.member.clone(),
                member_name,
                role: e.role,
                this_device: e.device.id == st.config.device,
                added_ms: s.map(|d| d.added_ms),
                last_seen_ms: s.and_then(|d| d.last_seen_ms),
            });
        }
        Ok(out)
    }

    /// Remove a device (an owner's action): the server stops serving it,
    /// and the household key rotates, so it cannot read anything new.
    pub fn remove_device(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        device: &str,
        recovery_key: Option<&str>,
    ) -> Result<()> {
        let target: DeviceId = device
            .parse()
            .map_err(|_| SyncError::Invalid("bad device id".into()))?;
        let st0 = State::load(store)?;
        if st0.config.role != DeviceRole::Owner {
            return Err(SyncError::Invalid(
                "Only an owner can remove devices.".into(),
            ));
        }
        if target == st0.config.device {
            return Err(SyncError::Invalid(
                "Use \"Stop syncing on this device\" to leave from this device.".into(),
            ));
        }
        // Removing an owner's device needs the recovery key (D-101).
        let cosigner = match st0.latest().find(target).map(|e| e.role) {
            Some(DeviceRole::Owner) => Some(recovery_cosigner(&st0, recovery_key)?),
            _ => None,
        };
        for attempt in 0..3 {
            // Catch up first, so we know the last batch the device sent.
            let _ = self.sync(store, blobs);
            let mut st = State::load(store)?;
            self.refresh(store, &mut st)?;
            let latest = st.latest().clone();
            let entry = latest
                .find(target)
                .filter(|e| e.role != DeviceRole::Recovery)
                .cloned()
                .ok_or_else(|| SyncError::Invalid("That device is not in the household.".into()))?;
            let head = st.heads.get(&target).cloned().unwrap_or_else(Head::start);
            let new_chain = st.chain.rotated()?;
            let mut next = latest.successor(st.rosters.last().expect("non-empty"), now_ms(store));
            next.devices.retain(|e| e.device.id != target);
            next.removed.push(RemovedEntry {
                entry,
                removed_in: next.version,
                last_seq: head.seq,
                last_hash: head.hash,
            });
            next.epoch += 1;
            next.sort();
            match self.publish_roster(
                store,
                &mut st,
                next,
                &new_chain,
                None,
                None,
                cosigner.as_ref(),
            ) {
                Ok(()) => {
                    st.chain = new_chain;
                    st.save(store)?;
                    return Ok(());
                }
                Err(SyncError::Server { code, .. })
                    if (code == "conflict" || code == "removal_stale") && attempt < 2 =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(SyncError::Invalid(
            "The household kept changing; try again in a moment.".into(),
        ))
    }

    /// Sign and send a new roster version with envelopes for `chain` to
    /// every device that needs them; on success, append it locally.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_roster(
        &mut self,
        store: &Store,
        st: &mut State,
        body: RosterBody,
        chain: &KeyChain,
        anchor: Option<Vec<u8>>,
        invitation: Option<String>,
        cosigner: Option<&DeviceSecret>,
    ) -> Result<()> {
        let prev = st.latest().clone();
        let mut signed = SignedRoster::sign(&body, &st.secret)?;
        if let Some(c) = cosigner {
            signed.cosign(c);
        }
        let body = verify_next(st.rosters.last().expect("non-empty"), &prev, &signed)?;
        let rotated = body.epoch != prev.epoch;
        let mut envelopes: Vec<KeyEnvelope> = Vec::new();
        for e in &body.devices {
            if rotated || prev.find(e.device.id).is_none() {
                envelopes.push(wrap(&st.secret, body.space, &e.device, chain)?);
            }
        }
        let req = UpdateRoster {
            roster: signed.clone(),
            envelopes,
            anchor,
            invitation,
        };
        let path = format!("/v1/s/{}/roster", body.space);
        let server = st.config.server.clone();
        self.call(
            store,
            &server,
            Some(&st.secret),
            "POST",
            &path,
            to_json(&req),
        )?;
        st.rosters.push(signed);
        st.bodies.push(body);
        Ok(())
    }

    /// Replace the recovery key (if the paper is lost or was seen). The
    /// old one stops working and the household key rotates.
    pub fn new_recovery_key(
        &mut self,
        store: &mut Store,
        current_key: &str,
    ) -> Result<Zeroizing<String>> {
        let mut st = State::load(store)?;
        if st.config.role != DeviceRole::Owner {
            return Err(SyncError::Invalid(
                "Only an owner can make a new recovery key.".into(),
            ));
        }
        self.refresh(store, &mut st)?;
        // The current key confirms it (D-101): a stolen phone alone cannot
        // lock the household's owner out.
        let cosigner = recovery_cosigner(&st, Some(current_key))?;
        let latest = st.latest().clone();
        let old = latest
            .recovery()
            .cloned()
            .ok_or_else(|| SyncError::Invalid("no recovery key".into()))?;
        let recovery = RecoveryKey::generate()?;
        let rec = recovery.identity();
        let new_chain = st.chain.rotated()?;
        let mut next = latest.successor(st.rosters.last().expect("non-empty"), now_ms(store));
        next.devices.retain(|e| e.device.id != old.device.id);
        next.removed.push(RemovedEntry {
            entry: old,
            removed_in: next.version,
            last_seq: 0,
            last_hash: ZERO_HASH.to_string(),
        });
        next.devices.push(RosterEntry {
            device: rec.public(),
            member: st.config.member.clone(),
            role: DeviceRole::Recovery,
            added_in: next.version,
            label: seal_label(&new_chain, latest.space, rec.id(), "Recovery key")?,
        });
        next.epoch += 1;
        next.sort();
        let genesis = st.rosters[0].hash();
        let anchor = recovery.seal_anchor(latest.space, &Anchor { genesis })?;
        self.publish_roster(
            store,
            &mut st,
            next,
            &new_chain,
            Some(anchor),
            None,
            Some(&cosigner),
        )?;
        st.chain = new_chain;
        st.status.recovery_confirmed = false;
        st.save(store)?;
        Ok(recovery.display())
    }

    /// On a fresh device: get everything back with the recovery key.
    pub fn restore(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        server: &str,
        recovery_text: &str,
        device_name: &str,
    ) -> Result<SyncReport> {
        if state::is_set_up(store)? || store.meta("household_id")?.is_some() {
            return Err(SyncError::Invalid(
                "Restoring needs a device with no household on it yet.".into(),
            ));
        }
        let server = check_server_url(server)?;
        let recovery = RecoveryKey::parse(recovery_text)?;
        let rid = recovery.identity();
        let r = self.call(store, &server, Some(&rid), "GET", "/v1/recovery", vec![])
            .map_err(|e| match e {
                SyncError::Server { code, .. } if code == "unknown_device" || code == "revoked" => SyncError::Invalid(
                    "This server does not know that recovery key. Check the key and the server address (a newer recovery key may have replaced it).".into(),
                ),
                e => e,
            })?;
        let view: RecoveryView = from_json(&r)?;
        let anchor = recovery.open_anchor(view.space, &view.anchor)?;
        let path = format!("/v1/s/{}/roster?after=0", view.space);
        let list: RosterList =
            from_json(&self.call(store, &server, Some(&rid), "GET", &path, vec![])?)?;
        let bodies = verify_chain(&list.rosters)?;
        if list.rosters[0].hash() != anchor.genesis {
            return Err(SyncError::Crypto(
                housewright_crypto::CryptoError::Rejected(
                    "the server's household history does not match this recovery key".into(),
                ),
            ));
        }
        let latest = bodies.last().expect("non-empty").clone();
        let rec_entry = latest
            .find(rid.id())
            .filter(|e| e.role == DeviceRole::Recovery)
            .cloned()
            .ok_or_else(|| {
                SyncError::Invalid("That recovery key was replaced by a newer one.".into())
            })?;
        let path = format!("/v1/s/{}/keys", view.space);
        let keys: KeyList =
            from_json(&self.call(store, &server, Some(&rid), "GET", &path, vec![])?)?;
        let env = keys
            .envelopes
            .iter()
            .max_by_key(|e| e.epoch)
            .ok_or_else(|| SyncError::Invalid("no keys were left for the recovery key".into()))?;
        let chain = open_envelope(&rid, &bodies, env)?;
        if chain.current_epoch() != latest.epoch {
            return Err(SyncError::Invalid(
                "the household's newest keys are missing".into(),
            ));
        }
        // Add this device, signed by the recovery identity.
        let secret = DeviceSecret::generate()?;
        let me = secret.public();
        let mut rec_state = State {
            config: SyncConfig {
                server: server.clone(),
                space: view.space,
                device: rid.id(),
                member: rec_entry.member.clone(),
                role: DeviceRole::Recovery,
            },
            secret: rid,
            chain: chain.clone(),
            rosters: list.rosters,
            bodies,
            cursor: 0,
            push_cursor: 0,
            next_device_seq: 1,
            acked_seq: 0,
            acked_upto: 0,
            heads: Default::default(),
            own_head: ZERO_HASH.to_string(),
            status: SyncStatus::default(),
        };
        let mut next =
            latest.successor(rec_state.rosters.last().expect("non-empty"), now_ms(store));
        next.devices.push(RosterEntry {
            device: me.clone(),
            member: rec_entry.member.clone(),
            role: DeviceRole::Owner,
            added_in: next.version,
            label: seal_label(&chain, view.space, me.id, device_name)?,
        });
        next.sort();
        self.publish_roster(store, &mut rec_state, next, &chain, None, None, None)?;
        let st = State {
            config: SyncConfig {
                server,
                space: view.space,
                device: me.id,
                member: rec_entry.member,
                role: DeviceRole::Owner,
            },
            secret,
            chain,
            rosters: rec_state.rosters.clone(),
            bodies: rec_state.bodies.clone(),
            cursor: 0,
            push_cursor: 0,
            next_device_seq: 1,
            acked_seq: 0,
            acked_upto: 0,
            heads: Default::default(),
            own_head: ZERO_HASH.to_string(),
            status: SyncStatus {
                recovery_confirmed: true,
                ..Default::default()
            },
        };
        st.save(store)?;
        self.sync(store, blobs)
    }
}

/// A sealed batch of this device, kept under `sent:<seq>` (D-107).
fn load_sent(store: &Store, seq: u64) -> Result<Pending> {
    let v = store.sync_get(&format!("sent:{seq:020}"))?.ok_or_else(|| {
        SyncError::Invalid(format!("this device's change batch {seq} is missing"))
    })?;
    serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))
}

fn save_sent(store: &Store, seq: u64, p: &Pending) -> Result<()> {
    store.sync_set(
        &format!("sent:{seq:020}"),
        &serde_json::to_string(p).map_err(|e| SyncError::Invalid(e.to_string()))?,
    )?;
    Ok(())
}

/// Keep a batch that cannot be applied yet: one per number. A second,
/// different batch under the same number is the device forking its own
/// chain, and is refused (N5). Returns whether it was newly kept.
fn hold(
    store: &Store,
    kind: &str,
    dev: DeviceId,
    seq: u64,
    p: &Pulled,
    hash: &str,
    report: &mut SyncReport,
) -> Result<bool> {
    let key = format!("{kind}:{dev}:{seq:020}");
    if let Some(v) = store.sync_get(&key)? {
        let old: Pulled =
            serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))?;
        let same = old.batch.as_ref().map(batch_hash).transpose()?.as_deref() == Some(hash);
        if !same {
            report.rejected_batches += 1;
            report.tampered += 1;
        }
        return Ok(false);
    }
    store.sync_set(
        &key,
        &serde_json::to_string(p).map_err(|e| SyncError::Invalid(e.to_string()))?,
    )?;
    report.held += 1;
    Ok(true)
}

fn device_name(st: &State, dev: DeviceId) -> String {
    let latest = st.latest();
    let e = latest
        .find(dev)
        .cloned()
        .or_else(|| latest.find_removed(dev).map(|r| r.entry.clone()));
    e.and_then(|e| open_label(&st.chain, latest.space, dev, &e.label).ok())
        .unwrap_or_else(|| "a device".into())
}

/// What sealing the next batch gave.
enum Sealed {
    Done,
    /// Only changes too big to send: skip past them.
    Skipped(i64),
    Batch(Box<SealedBatch>, i64, usize),
}

/// Seal the next batch of this device's changes after `cursor`.
#[allow(clippy::too_many_arguments)]
fn seal_next(
    store: &Store,
    secret: &DeviceSecret,
    chain: &KeyChain,
    space: SpaceId,
    roster: Option<SignedRoster>,
    seq: u64,
    prev: &str,
    cursor: i64,
    unsent_too_large: &mut u64,
) -> Result<Sealed> {
    let ops = store.unsynced_ops(cursor, BATCH_OPS)?;
    if ops.is_empty() {
        return Ok(Sealed::Done);
    }
    let mut chosen: Vec<Op> = Vec::new();
    let mut size = 0;
    let mut upto = cursor;
    for (n, op) in ops {
        let len = serde_json::to_vec(&op).map(|v| v.len()).unwrap_or(0);
        if len > MAX_OP_BYTES {
            // Cannot be sent at all (no screen makes one this big); it
            // stays on this device, and the person is told.
            if chosen.is_empty() {
                upto = n;
                *unsent_too_large += 1;
                continue;
            }
            break;
        }
        if !chosen.is_empty() && size + len > BATCH_BYTES {
            break;
        }
        size += len;
        chosen.push(op);
        upto = n;
    }
    if chosen.is_empty() {
        return Ok(Sealed::Skipped(upto));
    }
    let max_wall = chosen.iter().map(|o| o.hlc.wall_ms).max().unwrap_or(0);
    let body = BatchBody {
        roster,
        ops: serde_json::to_value(&chosen).map_err(|e| SyncError::Invalid(e.to_string()))?,
    };
    let plaintext = Zeroizing::new(to_json(&body));
    let batch = seal_batch(
        secret,
        chain,
        space,
        new_batch_id()?,
        seq,
        prev,
        max_wall,
        &plaintext,
    )?;
    Ok(Sealed::Batch(Box::new(batch), upto, chosen.len()))
}

/// Plain sentences for a roster change made elsewhere (M3): every other
/// device sees who was added or removed, and when the recovery key was
/// used, so a takeover cannot happen quietly.
fn describe_change(
    st: &State,
    prev: &RosterBody,
    next: &RosterBody,
    signed: &SignedRoster,
) -> Vec<String> {
    let name = |e: &RosterEntry| {
        open_label(&st.chain, next.space, e.device.id, &e.label)
            .unwrap_or_else(|_| "a device".into())
    };
    let by = prev
        .find(signed.signer)
        .map(|e| {
            if e.role == DeviceRole::Recovery {
                "the recovery key".to_string()
            } else {
                format!("“{}”", name(e))
            }
        })
        .unwrap_or_else(|| "another device".into());
    let mut out = Vec::new();
    for e in &next.devices {
        if prev.find(e.device.id).is_none() {
            if e.role == DeviceRole::Recovery {
                out.push(format!(
                    "A new recovery key was made by {by}. The old one no longer works."
                ));
            } else {
                out.push(format!("“{}” was added to the household by {by}.", name(e)));
            }
        }
    }
    for r in &next.removed {
        if prev.find_removed(r.entry.device.id).is_none() && r.entry.role != DeviceRole::Recovery {
            out.push(format!(
                "“{}” was removed from the household by {by}.",
                name(&r.entry)
            ));
        }
    }
    out
}

/// The recovery identity from a typed key, checked against the roster.
fn recovery_cosigner(st: &State, typed: Option<&str>) -> Result<DeviceSecret> {
    let typed = typed.ok_or_else(|| {
        SyncError::Invalid("This needs the household's recovery key (it is in the sealed volume of the house binder).".into())
    })?;
    let key = RecoveryKey::parse(typed)?;
    let id = key.identity();
    if st.latest().recovery().map(|r| r.device.id) != Some(id.id()) {
        return Err(SyncError::Invalid(
            "That is not this household's current recovery key.".into(),
        ));
    }
    Ok(id)
}

/// Who wrote a batch, according to the roster.
pub(crate) enum Author {
    Current(RosterEntry),
    Removed(RemovedEntry),
    Unknown,
}

pub(crate) fn author_of(latest: &RosterBody, id: DeviceId) -> Author {
    if let Some(e) = latest.find(id) {
        return Author::Current(e.clone());
    }
    match latest.find_removed(id) {
        Some(r) => Author::Removed(r.clone()),
        None => Author::Unknown,
    }
}

/// What opening a batch gave.
pub(crate) enum Opened {
    Ops(Vec<Op>),
    /// Authentic, but this version of the app cannot read it.
    Unreadable,
    /// Authentic (it is the device's own batch) but refused by a rule.
    Refused,
    /// Not the device's batch at all (bad signature or ciphertext).
    Forged,
}

/// Open an envelope, checking its sender was a manager in the roster
/// version that started its epoch.
pub(crate) fn open_envelope(
    me: &DeviceSecret,
    bodies: &[RosterBody],
    env: &KeyEnvelope,
) -> Result<KeyChain> {
    let first = bodies
        .iter()
        .find(|b| b.epoch == env.epoch)
        .ok_or_else(|| SyncError::Invalid("keys for an epoch the roster does not have".into()))?;
    let latest = bodies.last().expect("non-empty");
    let sender: DevicePublic = first
        .find(env.sender)
        .or_else(|| latest.find(env.sender))
        .filter(|e| e.role.can_manage())
        .map(|e| e.device.clone())
        .or_else(|| {
            // Wrapped by a manager added later in the same epoch.
            bodies
                .iter()
                .filter(|b| b.epoch == env.epoch)
                .find_map(|b| {
                    b.find(env.sender)
                        .filter(|e| e.role.can_manage())
                        .map(|e| e.device.clone())
                })
        })
        .ok_or_else(|| SyncError::Invalid("keys were not sent by an owner".into()))?;
    Ok(unwrap(me, env, &sender)?)
}

/// The batch type, re-exported for tests.
pub type Batch = SealedBatch;
