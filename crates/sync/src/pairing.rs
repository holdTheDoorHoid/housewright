//! Pairing a new device (D-23, D-94). See `housewright_crypto::pairing`
//! for the cryptography; this is the conversation.
//!
//! ```text
//! existing (owner) device E         server            new device N
//! invite_start: code, r, offer ──► invitation
//!                                   ◄── GET offer ── join_start (decrypt with code)
//!                                   ◄── join (sealed N keys)
//! invite_poll: sees N, reveal r ──►
//!                                   ◄── GET reveal ── join_poll: check commit
//! both show the safety number; the person compares them
//! invite_confirm(yes): roster + envelope ──► accepted
//!                                   ◄── GET state ── join_poll: accepted, sync
//! ```

use housewright_core::blobs::BlobStore;
use housewright_core::ids::MemberId;
use housewright_core::model::{Member, Role};
use housewright_core::store::Store;
use housewright_crypto::batch::ZERO_HASH;
use housewright_crypto::encoding::{hex, unhex_array};
use housewright_crypto::pairing::{PairingCode, Part, commitment, confirmation, safety_number};
use housewright_crypto::primitives::random;
use housewright_crypto::roster::{DeviceRole, RosterEntry, seal_label, verify_chain};
use housewright_crypto::wire::*;
use housewright_crypto::{DeviceSecret, KeyChain};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::client::{SyncReport, from_json, now_ms, open_envelope};
use crate::state::{self, State, SyncConfig, SyncStatus};
use crate::{Result, SyncClient, SyncError, check_server_url};

/// Invitations last ten minutes.
const INVITE_MS: u64 = 10 * 60 * 1000;

/// Who the invited device is for.
#[derive(Clone, Debug)]
pub enum Invitee {
    /// Another device of the person inviting.
    MyDevice,
    /// A new person in the household.
    NewPerson { name: String, role: Role },
}

pub(crate) struct InviteSession {
    code: PairingCode,
    r: Zeroizing<[u8; 32]>,
    role: DeviceRole,
    member: String,
    new_person: Option<(String, Role)>,
    expires_ms: u64,
    joiner: Option<Joiner>,
}

pub(crate) struct JoinSession {
    server: String,
    code: PairingCode,
    secret: DeviceSecret,
    offer: Offer,
    label: String,
    r: Option<[u8; 32]>,
    confirmed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InviteOut {
    /// `ABCD-EFGH-IJKL-MNOP-QRST-UVWX`: shown once, typed on the new device.
    pub code: String,
    pub server: String,
    /// What the QR code holds: `housewright-pair:<server>#<code>`.
    pub qr_text: String,
    pub expires_ms: u64,
}

/// Where a pairing stands, for either side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairState {
    /// Waiting for the other device.
    Waiting,
    /// Compare this number with the other screen.
    Compare {
        /// The new device has said its number matches (D-102). The owner's
        /// "yes" only counts after that.
        other_confirmed: bool,
        safety_number: String,
        other_name: String,
    },
    /// This side said yes; waiting for the other.
    WaitingForOther,
    /// Done: the device is in the household.
    Done,
    /// It did not work; start again.
    Failed { reason: String },
}

pub fn qr_text(server: &str, code: &str) -> String {
    format!("housewright-pair:{server}#{code}")
}

/// Read what a QR code (or a pasted link) holds.
pub fn parse_qr(text: &str) -> Option<(String, String)> {
    let rest = text.trim().strip_prefix("housewright-pair:")?;
    let (server, code) = rest.rsplit_once('#')?;
    Some((server.to_string(), code.to_string()))
}

impl SyncClient {
    // -- the existing device ----------------------------------------------

    pub fn invite_start(&mut self, store: &mut Store, who: Invitee) -> Result<InviteOut> {
        let mut st = State::load(store)?;
        if st.config.role != DeviceRole::Owner {
            return Err(SyncError::Invalid(
                "Only an owner can add devices and people.".into(),
            ));
        }
        self.refresh(store, &mut st)?;
        st.save(store)?;
        let (role, member, new_person) = match who {
            Invitee::MyDevice => (DeviceRole::Owner, st.config.member.clone(), None),
            Invitee::NewPerson { name, role } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(SyncError::Invalid("Say who you are adding.".into()));
                }
                (
                    crate::client::role_of(role),
                    MemberId::new().to_string(),
                    Some((name, role)),
                )
            }
        };
        let code = PairingCode::generate()?;
        let r = Zeroizing::new(random::<32>()?);
        let expires_ms = now_ms(store) + INVITE_MS;
        let offer = Offer {
            space: st.config.space,
            inviter: st.secret.public(),
            role,
            member: member.clone(),
            genesis: st.rosters[0].hash(),
            roster_version: st.latest().version,
            commit: hex(&commitment(&r)),
            household_name: store.household()?.name,
            expires_ms,
        };
        let sealed = code.seal(
            Part::Offer,
            &serde_json::to_vec(&offer).expect("serialises"),
        )?;
        let req = CreateInvitation {
            id: code.invitation_id(),
            offer: sealed,
            expires_ms,
        };
        let path = format!("/v1/s/{}/invitations", st.config.space);
        self.call(
            store,
            &st.config.server,
            Some(&st.secret),
            "POST",
            &path,
            serde_json::to_vec(&req).expect("serialises"),
        )?;
        let shown = code.display().to_string();
        self.invite = Some(InviteSession {
            code,
            r,
            role,
            member,
            new_person,
            expires_ms,
            joiner: None,
        });
        Ok(InviteOut {
            qr_text: qr_text(&st.config.server, &shown),
            code: shown,
            server: st.config.server.clone(),
            expires_ms,
        })
    }

    pub fn invite_poll(&mut self, store: &Store) -> Result<PairState> {
        let st = State::load(store)?;
        let Some(inv) = self.invite.as_mut() else {
            return Ok(PairState::Failed {
                reason: "No invitation is open.".into(),
            });
        };
        if now_ms(store) > inv.expires_ms {
            self.invite = None;
            return Ok(PairState::Failed {
                reason: "The invitation expired. Start again.".into(),
            });
        }
        let id = inv.code.invitation_id();
        let path = format!("/v1/s/{}/invitations/{id}", st.config.space);
        let server = st.config.server.clone();
        let res = self.call(store, &server, Some(&st.secret), "GET", &path, vec![]);
        let inv = self.invite.as_mut().expect("checked above");
        let res = match res {
            Ok(r) => r,
            Err(SyncError::Server { status: 404, .. }) => {
                self.invite = None;
                return Ok(PairState::Failed {
                    reason: "The invitation expired. Start again.".into(),
                });
            }
            Err(e) => return Err(e),
        };
        let s: InvitationState = from_json(&res)?;
        match s.state.as_str() {
            "open" => Ok(PairState::Waiting),
            "aborted" => {
                self.invite = None;
                Ok(PairState::Failed { reason: "The other device said the numbers did not match, so nothing was shared. Start again.".into() })
            }
            "joined" | "revealed" | "confirmed" => {
                if inv.joiner.is_none() {
                    let sealed = s
                        .request
                        .ok_or_else(|| SyncError::Invalid("no request".into()))?;
                    let joiner: Joiner = match inv
                        .code
                        .open(Part::Request, &sealed)
                        .ok()
                        .and_then(|p| serde_json::from_slice(&p).ok())
                    {
                        Some(j) => j,
                        None => {
                            self.invite = None;
                            return Ok(PairState::Failed {
                                reason:
                                    "The answer to the invitation could not be read. Start again."
                                        .into(),
                            });
                        }
                    };
                    joiner.device.check()?;
                    inv.joiner = Some(joiner);
                }
                if s.state == "joined" {
                    let reveal = inv.code.seal(Part::Reveal, inv.r.as_ref())?;
                    let path = format!("/v1/s/{}/invitations/{id}/reveal", st.config.space);
                    let body = serde_json::to_vec(&Reveal { reveal }).expect("serialises");
                    self.call(store, &server, Some(&st.secret), "POST", &path, body)?;
                }
                let inv = self.invite.as_ref().expect("still open");
                let j = inv.joiner.as_ref().expect("set above");
                let other_confirmed = s.state == "confirmed"
                    && s.confirm
                        .as_deref()
                        .and_then(|c| inv.code.open(Part::Confirm, c).ok())
                        .as_deref()
                        == Some(
                            confirmation(&id, &inv.r, &st.secret.public(), &j.device).as_slice(),
                        );
                Ok(PairState::Compare {
                    other_confirmed,
                    safety_number: safety_number(&id, &inv.r, &st.secret.public(), &j.device),
                    other_name: j.label.clone(),
                })
            }
            "accepted" => Ok(PairState::Done),
            other => Ok(PairState::Failed {
                reason: format!("Unexpected invitation state '{other}'."),
            }),
        }
    }

    /// The person compared the numbers. Yes: add the device and give it
    /// the household key. No: cancel, nothing is shared.
    pub fn invite_confirm(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        matches: bool,
    ) -> Result<PairState> {
        let mut st = State::load(store)?;
        let Some(inv) = self.invite.take() else {
            return Ok(PairState::Failed {
                reason: "No invitation is open.".into(),
            });
        };
        let id = inv.code.invitation_id();
        let server = st.config.server.clone();
        if !matches {
            let path = format!("/v1/s/{}/invitations/{id}", st.config.space);
            let _ = self.call(store, &server, Some(&st.secret), "DELETE", &path, vec![]);
            return Ok(PairState::Failed {
                reason: "Cancelled. Nothing was shared with the other device.".into(),
            });
        }
        // The new device must have said "they match" first (D-102).
        let path = format!("/v1/s/{}/invitations/{id}", st.config.space);
        let state: InvitationState =
            from_json(&self.call(store, &server, Some(&st.secret), "GET", &path, vec![])?)?;
        let confirmed = state.state == "confirmed"
            && state
                .confirm
                .as_deref()
                .and_then(|c| inv.code.open(Part::Confirm, c).ok())
                .as_deref()
                == inv
                    .joiner
                    .as_ref()
                    .map(|j| confirmation(&id, &inv.r, &st.secret.public(), &j.device).to_vec())
                    .as_deref();
        if !confirmed {
            self.invite = Some(inv);
            return Err(SyncError::Invalid(
                "Wait until the new device has said the numbers match too.".into(),
            ));
        }
        let joiner = inv
            .joiner
            .clone()
            .ok_or_else(|| SyncError::Invalid("The other device has not answered yet.".into()))?;
        if let Some((name, role)) = &inv.new_person {
            let member = Member {
                id: inv
                    .member
                    .parse()
                    .map_err(|_| SyncError::Invalid("bad member".into()))?,
                name: name.clone(),
                role: *role,
                contact: None,
                reminders: None,
            };
            if store.get::<Member>(&inv.member)?.is_none() {
                store.put(&member)?;
            }
            // Send the new person's record before adding their device, so
            // it is on the server when the new device first syncs.
            self.sync(store, blobs)?;
            st = State::load(store)?;
        }
        for attempt in 0..2 {
            self.refresh(store, &mut st)?;
            let latest = st.latest().clone();
            let chain = st.chain.clone();
            let mut next = latest.successor(st.rosters.last().expect("non-empty"), now_ms(store));
            next.devices.push(RosterEntry {
                device: joiner.device.clone(),
                member: inv.member.clone(),
                role: inv.role,
                added_in: next.version,
                label: seal_label(&chain, latest.space, joiner.device.id, &joiner.label)?,
            });
            next.sort();
            match self.publish_roster(store, &mut st, next, &chain, None, Some(id.clone()), None) {
                Ok(()) => break,
                Err(SyncError::Server { code, .. }) if code == "conflict" && attempt == 0 => {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        st.save(store)?;
        // Send the new person's member record (and anything else) now, so
        // the new device finds it on its first sync.
        let _ = self.sync(store, blobs);
        Ok(PairState::Done)
    }

    pub fn invite_cancel(&mut self, store: &Store) -> Result<()> {
        if let Some(inv) = self.invite.take() {
            let st = State::load(store)?;
            let path = format!(
                "/v1/s/{}/invitations/{}",
                st.config.space,
                inv.code.invitation_id()
            );
            let _ = self.call(
                store,
                &st.config.server,
                Some(&st.secret),
                "DELETE",
                &path,
                vec![],
            );
        }
        Ok(())
    }

    // -- the new device ---------------------------------------------------

    pub fn join_start(
        &mut self,
        store: &Store,
        server: &str,
        code: &str,
        device_name: &str,
    ) -> Result<PairState> {
        if state::is_set_up(store)? || store.meta("household_id")?.is_some() {
            return Err(SyncError::Invalid(
                "This device already has a household. Joining needs a device with nothing on it yet.".into(),
            ));
        }
        let server = check_server_url(server)?;
        let code = PairingCode::parse(code)?;
        let path = format!("/v1/invitations/{}", code.invitation_id());
        let view: InvitationView =
            from_json(&self.call(store, &server, None, "GET", &path, vec![])?)?;
        let offer: Offer = match code.open(Part::Offer, &view.offer) {
            Ok(p) => serde_json::from_slice(&p)
                .map_err(|_| SyncError::Invalid("unreadable invitation".into()))?,
            Err(_) => {
                return Err(SyncError::Invalid(
                    "That code doesn't match an invitation on this server. Check it and try again."
                        .into(),
                ));
            }
        };
        if view.state != "open" {
            return Err(SyncError::Invalid(
                "Another device has already answered that invitation. Ask for a new code.".into(),
            ));
        }
        offer.inviter.check()?;
        let secret = DeviceSecret::generate()?;
        let joiner = Joiner {
            device: secret.public(),
            label: device_name.trim().to_string(),
        };
        let request = code.seal(
            Part::Request,
            &serde_json::to_vec(&joiner).expect("serialises"),
        )?;
        let path = format!("/v1/invitations/{}/join", code.invitation_id());
        self.call(
            store,
            &server,
            None,
            "POST",
            &path,
            serde_json::to_vec(&JoinRequest { request }).expect("serialises"),
        )?;
        self.join = Some(JoinSession {
            server,
            code,
            secret,
            offer,
            label: joiner.label,
            r: None,
            confirmed: false,
        });
        Ok(PairState::Waiting)
    }

    pub fn join_poll(&mut self, store: &mut Store, blobs: &mut dyn BlobStore) -> Result<PairState> {
        let Some(j) = self.join.as_ref() else {
            return Ok(PairState::Failed {
                reason: "Nothing is being joined.".into(),
            });
        };
        let path = format!("/v1/invitations/{}", j.code.invitation_id());
        let server = j.server.clone();
        let view: InvitationView = match self.call(store, &server, None, "GET", &path, vec![]) {
            Ok(r) => from_json(&r)?,
            Err(SyncError::Server { status: 404, .. }) => {
                self.join = None;
                return Ok(PairState::Failed {
                    reason: "The invitation expired or was cancelled on the other device.".into(),
                });
            }
            Err(e) => return Err(e),
        };
        let j = self.join.as_mut().expect("checked above");
        if view.state == "aborted" {
            self.join = None;
            return Ok(PairState::Failed {
                reason: "The pairing was cancelled.".into(),
            });
        }
        if j.r.is_none() {
            let Some(sealed) = view.reveal else {
                return Ok(PairState::Waiting);
            };
            let r: [u8; 32] = j
                .code
                .open(Part::Reveal, &sealed)
                .ok()
                .and_then(|v| v.try_into().ok())
                .ok_or_else(|| SyncError::Invalid("unreadable answer".into()))?;
            if hex(&commitment(&r)) != j.offer.commit {
                let _ = self.call(
                    store,
                    &server,
                    None,
                    "POST",
                    &format!("{path}/abort"),
                    vec![],
                );
                self.join = None;
                return Ok(PairState::Failed {
                    reason: "Something interfered with the pairing, so it was stopped. Nothing was shared. Try again.".into(),
                });
            }
            j.r = Some(r);
        }
        let j = self.join.as_ref().expect("still joining");
        if !j.confirmed {
            return Ok(PairState::Compare {
                other_confirmed: false,
                safety_number: safety_number(
                    &j.code.invitation_id(),
                    j.r.as_ref().expect("set above"),
                    &j.offer.inviter,
                    &j.secret.public(),
                ),
                other_name: j.offer.household_name.clone(),
            });
        }
        if view.state != "accepted" {
            return Ok(PairState::WaitingForOther);
        }
        let j = self.join.take().expect("still joining");
        self.finish_join(store, blobs, j)?;
        Ok(PairState::Done)
    }

    pub fn join_confirm(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        matches: bool,
    ) -> Result<PairState> {
        let Some(j) = self.join.as_mut() else {
            return Ok(PairState::Failed {
                reason: "Nothing is being joined.".into(),
            });
        };
        if !matches {
            let path = format!("/v1/invitations/{}/abort", j.code.invitation_id());
            let server = j.server.clone();
            let _ = self.call(store, &server, None, "POST", &path, vec![]);
            self.join = None;
            return Ok(PairState::Failed {
                reason: "Stopped. Nothing was shared. Try again with a new code.".into(),
            });
        }
        if j.r.is_none() {
            return Ok(PairState::Waiting);
        }
        if !j.confirmed {
            // Bound to exactly what the safety number was made from (D-111).
            let digest = confirmation(
                &j.code.invitation_id(),
                j.r.as_ref().expect("checked above"),
                &j.offer.inviter,
                &j.secret.public(),
            );
            let confirm = j.code.seal(Part::Confirm, &digest)?;
            let path = format!("/v1/invitations/{}/confirm", j.code.invitation_id());
            let server = j.server.clone();
            let body = serde_json::to_vec(&JoinConfirm { confirm }).expect("serialises");
            self.call(store, &server, None, "POST", &path, body)?;
            if let Some(j) = self.join.as_mut() {
                j.confirmed = true;
            }
        }
        self.join_poll(store, blobs)
    }

    fn finish_join(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        j: JoinSession,
    ) -> Result<SyncReport> {
        let space = j.offer.space;
        let path = format!("/v1/s/{space}/roster?after=0");
        let list: RosterList =
            from_json(&self.call(store, &j.server, Some(&j.secret), "GET", &path, vec![])?)?;
        let bodies = verify_chain(&list.rosters)?;
        let bad = |m: &str| SyncError::Crypto(housewright_crypto::CryptoError::Rejected(m.into()));
        if list.rosters[0].hash() != j.offer.genesis {
            return Err(bad(
                "the household history is not the one the invitation named",
            ));
        }
        let latest = bodies.last().expect("non-empty").clone();
        if latest.version < j.offer.roster_version {
            return Err(bad("the server showed an old household list"));
        }
        let me = j.secret.public();
        let mine = latest
            .find(me.id)
            .ok_or_else(|| bad("this device was not added"))?
            .clone();
        let adder = list
            .rosters
            .iter()
            .zip(&bodies)
            .find(|(_, b)| b.version == mine.added_in)
            .map(|(s, _)| s.signer);
        if adder != Some(j.offer.inviter.id)
            || mine.role != j.offer.role
            || mine.member != j.offer.member
        {
            return Err(bad(
                "this device was added by someone other than the device that invited it",
            ));
        }
        let path = format!("/v1/s/{space}/keys");
        let keys: KeyList =
            from_json(&self.call(store, &j.server, Some(&j.secret), "GET", &path, vec![])?)?;
        let env = keys
            .envelopes
            .iter()
            .max_by_key(|e| e.epoch)
            .ok_or_else(|| bad("no keys arrived"))?;
        let chain: KeyChain = open_envelope(&j.secret, &bodies, env)?;
        let st = State {
            config: SyncConfig {
                server: j.server.clone(),
                space,
                device: me.id,
                member: j.offer.member.clone(),
                role: j.offer.role,
            },
            secret: j.secret.clone(),
            chain,
            rosters: list.rosters,
            bodies,
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
        let _ = unhex_array::<16>;
        let _ = &j.label;
        self.sync(store, blobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_round_trip() {
        let t = qr_text("https://hw.example.org", "ABCD-EFGH");
        assert_eq!(
            parse_qr(&t),
            Some(("https://hw.example.org".into(), "ABCD-EFGH".into()))
        );
        assert_eq!(parse_qr("nonsense"), None);
    }
}
