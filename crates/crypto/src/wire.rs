//! The sync protocol's JSON bodies, shared by the client and the server so
//! the two cannot drift apart. Paths are listed in `docs/SECURITY.md`.

use serde::{Deserialize, Serialize};

use crate::batch::SealedBatch;
use crate::keys::b64_vec;
use crate::roster::{DeviceRole, SignedRoster};
use crate::wrap::KeyEnvelope;
use crate::{DeviceId, DevicePublic, SpaceId};

/// Error body: `{"error": {"code", "message", "server_time_ms"?}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_time_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Health {
    pub ok: bool,
    pub version: String,
    pub time_ms: u64,
    pub registration_open: bool,
}

/// `POST /v1/spaces`, signed by the founding device.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSpace {
    pub space: SpaceId,
    pub roster: SignedRoster,
    /// One for the founder, one for the recovery identity.
    pub envelopes: Vec<KeyEnvelope>,
    #[serde(with = "b64_vec")]
    pub anchor: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_token: Option<String>,
}

/// `GET /v1/s/{space}/roster?after=N`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RosterList {
    pub rosters: Vec<SignedRoster>,
}

/// `POST /v1/s/{space}/roster`: a new roster version (add a device,
/// remove devices and rotate, replace the recovery key). Owners only.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateRoster {
    pub roster: SignedRoster,
    /// Envelopes at the new roster's epoch: for every new device, and when
    /// the epoch changes, for every device in the roster.
    pub envelopes: Vec<KeyEnvelope>,
    /// A new anchor (only when the recovery identity changes).
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_b64")]
    pub anchor: Option<Vec<u8>>,
    /// Close this invitation (pairing finished).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
    /// A version the server lost (it was restored from a backup), sent
    /// again by an owner's device (D-107, D-114).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restore: bool,
}

/// `GET /v1/s/{space}/keys`: the caller's envelopes, every epoch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyList {
    pub envelopes: Vec<KeyEnvelope>,
}

/// `POST /v1/s/{space}/ops`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushOps {
    pub batches: Vec<SealedBatch>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Accepted {
    pub batch_id: String,
    pub seq: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushResult {
    pub accepted: Vec<Accepted>,
}

/// One stored batch, as pulled.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pulled {
    pub seq: u64,
    /// When the server received it (its own clock): the clock guard's
    /// reference, the same for every receiver (D-93).
    pub received_ms: u64,
    /// Absent when the stored batch is larger than the server will send
    /// (`oversize`); the client skips it and says so (D-100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<SealedBatch>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub oversize: bool,
    /// With `oversize`: whose batch it was, and its number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<crate::DeviceId>,
    #[serde(default)]
    pub device_seq: u64,
}

/// `GET /v1/s/{space}/ops?after=S&limit=L`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullOps {
    pub batches: Vec<Pulled>,
    pub more: bool,
    /// The server's newest batch number for the household: a device whose
    /// cursor is past it is talking to a server restored from a backup
    /// (D-107).
    #[serde(default)]
    pub head: u64,
    /// The server's newest roster version.
    #[serde(default)]
    pub roster_version: u64,
}

/// `GET /v1/s/{space}/chain/{device}/{seq}`: one batch of a device, by its
/// own number (to fill a gap in its chain, D-107).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OneBatch {
    pub batch: Pulled,
}

/// `GET /v1/s/{space}/chain/{device}`: the newest number the server holds
/// for a device (to re-send after a restore).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainHead {
    pub seq: u64,
    /// Numbers below `seq` the server does not hold (it lost them), at
    /// most 1000: the device sends them again (D-107).
    #[serde(default)]
    pub missing: Vec<u64>,
}

/// `POST /v1/s/{space}/blobs/missing`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobNames {
    pub names: Vec<String>,
}

/// `GET /v1/s/{space}/devices`: what the server knows about each device.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceSeen {
    pub id: DeviceId,
    pub role: DeviceRole,
    pub added_ms: u64,
    pub last_seen_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceList {
    pub devices: Vec<DeviceSeen>,
}

/// `POST /v1/s/{space}/invitations` (owners).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateInvitation {
    pub id: String,
    #[serde(with = "b64_vec")]
    pub offer: Vec<u8>,
    pub expires_ms: u64,
}

/// `GET /v1/invitations/{id}` (anyone with the id; rate limited).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvitationView {
    #[serde(with = "b64_vec")]
    pub offer: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_b64")]
    pub reveal: Option<Vec<u8>>,
    /// open, joined, revealed, accepted, aborted, expired
    pub state: String,
}

/// `POST /v1/invitations/{id}/join`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JoinRequest {
    #[serde(with = "b64_vec")]
    pub request: Vec<u8>,
}

/// `POST /v1/invitations/{id}/confirm`: the new device says the numbers
/// match (sealed with the pairing code, D-102).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JoinConfirm {
    #[serde(with = "b64_vec")]
    pub confirm: Vec<u8>,
}

/// `GET /v1/s/{space}/invitations/{id}` (owners).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvitationState {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_b64")]
    pub request: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_b64")]
    pub confirm: Option<Vec<u8>>,
}

/// `POST /v1/s/{space}/invitations/{id}/reveal` (owners).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reveal {
    #[serde(with = "b64_vec")]
    pub reveal: Vec<u8>,
}

/// `GET /v1/recovery`, signed by the recovery identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryView {
    pub space: SpaceId,
    #[serde(with = "b64_vec")]
    pub anchor: Vec<u8>,
}

/// Inside the sealed offer (never seen by the server).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub space: SpaceId,
    pub inviter: DevicePublic,
    pub role: DeviceRole,
    /// The member the new device will act as.
    pub member: String,
    pub genesis: String,
    pub roster_version: u64,
    /// `SHA-256` commitment to the inviter's safety-number value (hex).
    pub commit: String,
    pub household_name: String,
    pub expires_ms: u64,
}

/// What a sealed batch holds: the author's newest roster version, signed
/// (so devices notice a roster the server is hiding from them), then the
/// operations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchBody {
    /// The author's newest signed roster versions, oldest first (at most
    /// eight; D-106, revised D-112). A receiver believes a newer version
    /// only if these link to its own verified chain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rosters: Vec<SignedRoster>,
    pub ops: serde_json::Value,
}

/// Inside the sealed join request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Joiner {
    pub device: DevicePublic,
    pub label: String,
}

pub mod opt_b64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_str(&crate::encoding::b64(b)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let o: Option<String> = Option::deserialize(d)?;
        o.map(|s| crate::encoding::unb64(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}
