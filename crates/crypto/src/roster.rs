//! The roster: the signed list of devices in a household (D-92).
//!
//! The server must not decide who is in the household: if it could, it
//! could slip its own key in and be sent the household key at the next
//! rotation. So the device list is a hash chain of signed versions:
//!
//! * Version 1 (genesis) is signed by the founding owner device and lists
//!   it and the recovery identity, at epoch 0.
//! * Version n+1 names the hash of version n, and must be signed by a
//!   device that version n lists as an owner (or the recovery identity).
//! * Entries never change once added. A removed device moves to `removed`,
//!   with the highest batch number its remover had seen from it; removing
//!   anyone forces a new epoch.
//! * Every client checks every step ([`verify_next`]); the server checks
//!   too, but nobody relies on it.
//!
//! A new device trusts the chain because the version that adds it is
//! signed by the device it paired with (whose key arrived inside the
//! pairing exchange), and the offer named the genesis hash. A restoring
//! device trusts it because the anchor sealed with the recovery key names
//! the genesis hash ([`crate::recovery`]).
//!
//! Device names are household data, so each entry's label is encrypted
//! under the label key of the epoch in which the device was added.
//! Everything else in the roster (ids, public keys, member ids, roles) is
//! visible to the server; it needs most of it anyway.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::encoding::hex;
use crate::keys::{Purpose, b64_64, b64_vec};
use crate::primitives::{open, seal, sha256, transcript};
use crate::{CryptoError, DeviceId, DevicePublic, DeviceSecret, KeyChain, Result, SpaceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceRole {
    Owner,
    Member,
    Limited,
    /// The identity derived from the printed recovery key.
    Recovery,
}

impl DeviceRole {
    /// May add and remove devices and rotate keys.
    pub fn can_manage(self) -> bool {
        matches!(self, DeviceRole::Owner | DeviceRole::Recovery)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceRole::Owner => "owner",
            DeviceRole::Member => "member",
            DeviceRole::Limited => "limited",
            DeviceRole::Recovery => "recovery",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "owner" => DeviceRole::Owner,
            "member" => DeviceRole::Member,
            "limited" => DeviceRole::Limited,
            "recovery" => DeviceRole::Recovery,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub device: DevicePublic,
    /// The household member (a member id in the data) this device acts as.
    pub member: String,
    pub role: DeviceRole,
    pub added_in: u64,
    /// The device's name, sealed (see [`seal_label`]).
    #[serde(with = "b64_vec")]
    pub label: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovedEntry {
    pub entry: RosterEntry,
    pub removed_in: u64,
    /// Batches from this device numbered above this are refused by everyone.
    pub last_seq: u64,
    /// The hash of its batch number `last_seq` (64 zeros if none): the end
    /// of its batch chain. Only batches on the chain that ends here are
    /// accepted from it, so it cannot write anything new (D-99).
    pub last_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterBody {
    pub v: u8,
    pub space: SpaceId,
    pub version: u64,
    pub epoch: u32,
    /// Hash of the previous signed version (hex); 64 zeros for genesis.
    pub prev: String,
    /// Hash of the genesis version (hex); empty in genesis itself.
    pub genesis: String,
    /// Sorted by device id.
    pub devices: Vec<RosterEntry>,
    pub removed: Vec<RemovedEntry>,
    pub made_ms: u64,
    /// Genesis only (D-110): how many of the founding device's first
    /// batches carry the household's history from before sync, and the
    /// hash of the last of them. Only those batches may hold operations in
    /// other members' names.
    #[serde(default)]
    pub history_batches: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub history_head: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRoster {
    /// The exact JSON bytes that were signed.
    #[serde(with = "b64_vec")]
    pub body: Vec<u8>,
    pub signer: DeviceId,
    #[serde(with = "b64_64")]
    pub sig: [u8; 64],
    /// A second signature by the recovery identity of the previous
    /// version, needed to remove an owner's device or replace the recovery
    /// key (D-101): the person must type the recovery key.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_sig")]
    pub cosig: Option<[u8; 64]>,
}

mod opt_sig {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<[u8; 64]>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_str(&crate::encoding::b64(b)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<[u8; 64]>, D::Error> {
        let o: Option<String> = Option::deserialize(d)?;
        o.map(|s| crate::encoding::unb64_array(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn bad(msg: &str) -> CryptoError {
    CryptoError::Rejected(format!("roster: {msg}"))
}

impl SignedRoster {
    pub fn sign(body: &RosterBody, signer: &DeviceSecret) -> Result<Self> {
        let bytes = serde_json::to_vec(body).map_err(|e| CryptoError::Malformed(e.to_string()))?;
        let sig = signer.sign(&transcript("housewright/v1/roster-sig", &[&bytes]));
        Ok(Self {
            body: bytes,
            signer: signer.id(),
            sig,
            cosig: None,
        })
    }

    /// Add the recovery identity's second signature (D-101).
    pub fn cosign(&mut self, recovery: &DeviceSecret) {
        self.cosig = Some(recovery.sign(&transcript("housewright/v1/roster-cosig", &[&self.body])));
    }

    pub fn parse(&self) -> Result<RosterBody> {
        serde_json::from_slice(&self.body).map_err(|e| CryptoError::Malformed(e.to_string()))
    }

    /// `SHA-256(transcript("housewright/v1/roster-hash", body, signer, sig, cosig or empty))`, hex.
    pub fn hash(&self) -> String {
        let cosig: &[u8] = match &self.cosig {
            Some(c) => c,
            None => &[],
        };
        hex(&sha256(&transcript(
            "housewright/v1/roster-hash",
            &[&self.body, &self.signer.0, &self.sig, cosig],
        )))
    }

    /// Check the main signature against a given device's key.
    pub fn check_sig(&self, signer: &DevicePublic) -> Result<()> {
        signer.verify(
            &transcript("housewright/v1/roster-sig", &[&self.body]),
            &self.sig,
        )
    }
}

impl RosterBody {
    pub fn find(&self, id: DeviceId) -> Option<&RosterEntry> {
        self.devices.iter().find(|e| e.device.id == id)
    }

    pub fn find_removed(&self, id: DeviceId) -> Option<&RemovedEntry> {
        self.removed.iter().find(|e| e.entry.device.id == id)
    }

    pub fn recovery(&self) -> Option<&RosterEntry> {
        self.devices.iter().find(|e| e.role == DeviceRole::Recovery)
    }

    /// A copy for the next version: same everything, version + 1, linked.
    pub fn successor(&self, signed: &SignedRoster, made_ms: u64) -> RosterBody {
        let mut n = self.clone();
        n.version += 1;
        n.history_batches = 0;
        n.history_head = String::new();
        n.prev = signed.hash();
        if self.version == 1 {
            n.genesis = signed.hash();
        }
        n.made_ms = made_ms;
        n
    }

    pub fn sort(&mut self) {
        self.devices.sort_by_key(|e| e.device.id);
        self.removed.sort_by_key(|e| e.entry.device.id);
    }
}

fn check_devices(b: &RosterBody) -> Result<()> {
    if b.v != 1 {
        return Err(bad("unknown format"));
    }
    let mut last: Option<DeviceId> = None;
    for e in &b.devices {
        e.device.check()?;
        if last.is_some_and(|l| l >= e.device.id) {
            return Err(bad("devices not sorted or repeated"));
        }
        last = Some(e.device.id);
    }
    if b.devices
        .iter()
        .filter(|e| e.role == DeviceRole::Recovery)
        .count()
        != 1
    {
        return Err(bad("needs exactly one recovery identity"));
    }
    if !b.devices.iter().any(|e| e.role == DeviceRole::Owner) {
        return Err(bad("needs at least one owner device"));
    }
    if b.removed
        .iter()
        .any(|r| b.find(r.entry.device.id).is_some())
    {
        return Err(bad("a device is both listed and removed"));
    }
    Ok(())
}

/// Check the first version.
pub fn verify_genesis(s: &SignedRoster) -> Result<RosterBody> {
    let b = s.parse()?;
    if b.version != 1
        || b.epoch != 0
        || b.prev != ZERO_HASH
        || !b.genesis.is_empty()
        || (b.history_batches == 0) != b.history_head.is_empty()
    {
        return Err(bad("not a valid first version"));
    }
    if !b.removed.is_empty() || b.devices.iter().any(|e| e.added_in != 1) {
        return Err(bad("not a valid first version"));
    }
    check_devices(&b)?;
    let signer = b
        .find(s.signer)
        .filter(|e| e.role == DeviceRole::Owner)
        .ok_or_else(|| bad("first version not signed by its owner"))?;
    s.check_sig(&signer.device)?;
    Ok(b)
}

/// Check that `next` is a valid successor of `prev`.
pub fn verify_next(
    prev_signed: &SignedRoster,
    prev: &RosterBody,
    next: &SignedRoster,
) -> Result<RosterBody> {
    let n = next.parse()?;
    if n.space != prev.space
        || n.version != prev.version + 1
        || n.history_batches != 0
        || !n.history_head.is_empty()
    {
        return Err(bad("out of sequence"));
    }
    if n.prev != prev_signed.hash() {
        return Err(bad("does not follow the previous version"));
    }
    let genesis = if prev.version == 1 {
        prev_signed.hash()
    } else {
        prev.genesis.clone()
    };
    if n.genesis != genesis {
        return Err(bad("wrong household history"));
    }
    let signer = prev
        .find(next.signer)
        .filter(|e| e.role.can_manage())
        .ok_or_else(|| bad("not signed by an owner"))?;
    next.check_sig(&signer.device)?;
    check_devices(&n)?;
    if n.removed
        .iter()
        .any(|r| r.last_hash.len() != 64 || r.last_hash.bytes().any(|b| !b.is_ascii_hexdigit()))
    {
        return Err(bad("bad removal record"));
    }

    let old: BTreeMap<DeviceId, &RemovedEntry> = prev
        .removed
        .iter()
        .map(|r| (r.entry.device.id, r))
        .collect();
    for r in &prev.removed {
        if n.find_removed(r.entry.device.id) != Some(r) {
            return Err(bad("a removal was undone or changed"));
        }
    }
    let mut newly_removed = 0;
    for r in &n.removed {
        if old.contains_key(&r.entry.device.id) {
            continue;
        }
        if prev.find(r.entry.device.id) != Some(&r.entry) || r.removed_in != n.version {
            return Err(bad("removed a device that was not there"));
        }
        newly_removed += 1;
    }
    for e in &prev.devices {
        let kept = n.find(e.device.id);
        let gone = n.find_removed(e.device.id).is_some();
        match (kept, gone) {
            (Some(k), false) if k == e => {}
            (None, true) => {}
            _ => return Err(bad("an existing device was changed")),
        }
    }
    for e in &n.devices {
        if prev.find(e.device.id).is_none()
            && (e.added_in != n.version || old.contains_key(&e.device.id))
        {
            return Err(bad("bad new device"));
        }
    }
    // D-101: the recovery identity may only add devices; removing an
    // owner's device or the recovery identity needs the recovery key's
    // second signature.
    let removed_now: Vec<&RemovedEntry> = n
        .removed
        .iter()
        .filter(|r| !old.contains_key(&r.entry.device.id))
        .collect();
    if signer.role == DeviceRole::Recovery
        && (!removed_now.is_empty()
            || n.recovery().map(|r| r.device.id) != prev.recovery().map(|r| r.device.id))
    {
        return Err(bad("the recovery key can only add a device"));
    }
    if removed_now
        .iter()
        .any(|r| matches!(r.entry.role, DeviceRole::Owner | DeviceRole::Recovery))
    {
        let rec = prev.recovery().ok_or_else(|| bad("no recovery identity"))?;
        let cosig = next
            .cosig
            .ok_or_else(|| bad("removing an owner's device needs the recovery key"))?;
        rec.device
            .verify(
                &transcript("housewright/v1/roster-cosig", &[&next.body]),
                &cosig,
            )
            .map_err(|_| bad("the recovery key's confirmation did not verify"))?;
    }
    let epoch_ok = if newly_removed > 0 {
        n.epoch == prev.epoch + 1
    } else {
        n.epoch == prev.epoch || n.epoch == prev.epoch + 1
    };
    if !epoch_ok {
        return Err(bad("removing a device must start a new epoch"));
    }
    Ok(n)
}

/// Check a whole chain from genesis; returns every body.
pub fn verify_chain(chain: &[SignedRoster]) -> Result<Vec<RosterBody>> {
    let first = chain.first().ok_or_else(|| bad("empty"))?;
    let mut bodies = vec![verify_genesis(first)?];
    for w in chain.windows(2) {
        let prev = bodies.last().expect("non-empty");
        let next = verify_next(&w[0], prev, &w[1])?;
        bodies.push(next);
    }
    Ok(bodies)
}

/// A device name sealed for the roster: `u32be(epoch) || nonce || ct`.
pub fn seal_label(
    chain: &KeyChain,
    space: SpaceId,
    device: DeviceId,
    name: &str,
) -> Result<Vec<u8>> {
    let epoch = chain.current_epoch();
    let key = chain.key(space, epoch, Purpose::Labels)?;
    let mut out = epoch.to_be_bytes().to_vec();
    out.extend(seal(
        &key,
        name.as_bytes(),
        &transcript("housewright/v1/label", &[&device.0]),
    )?);
    Ok(out)
}

pub fn open_label(
    chain: &KeyChain,
    space: SpaceId,
    device: DeviceId,
    label: &[u8],
) -> Result<String> {
    if label.len() < 4 {
        return Err(CryptoError::Decrypt);
    }
    let epoch = u32::from_be_bytes(label[..4].try_into().expect("4 bytes"));
    let key = chain.key(space, epoch, Purpose::Labels)?;
    let pt = open(
        &key,
        &label[4..],
        &transcript("housewright/v1/label", &[&device.0]),
    )?;
    String::from_utf8(pt).map_err(|_| CryptoError::Malformed("label is not text".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn entry(d: &DeviceSecret, role: DeviceRole, v: u64) -> RosterEntry {
        RosterEntry {
            device: d.public(),
            member: "m1".into(),
            role,
            added_in: v,
            label: vec![0; 4],
        }
    }

    fn genesis(
        owner: &DeviceSecret,
        rec: &DeviceSecret,
        space: SpaceId,
    ) -> (SignedRoster, RosterBody) {
        let mut b = RosterBody {
            v: 1,
            space,
            version: 1,
            epoch: 0,
            prev: ZERO_HASH.into(),
            genesis: String::new(),
            devices: vec![
                entry(owner, DeviceRole::Owner, 1),
                entry(rec, DeviceRole::Recovery, 1),
            ],
            removed: vec![],
            made_ms: 1,
            history_batches: 0,
            history_head: String::new(),
        };
        b.sort();
        let s = SignedRoster::sign(&b, owner).unwrap();
        (s, b)
    }

    #[test]
    fn chain_add_and_remove() {
        let owner = DeviceSecret::generate().unwrap();
        let rec = DeviceSecret::generate().unwrap();
        let phone = DeviceSecret::generate().unwrap();
        let space = SpaceId::random().unwrap();
        let (g, gb) = genesis(&owner, &rec, space);
        assert_eq!(verify_genesis(&g).unwrap(), gb);

        // Add a member's phone.
        let mut b2 = gb.successor(&g, 2);
        b2.devices.push(entry(&phone, DeviceRole::Member, 2));
        b2.sort();
        let s2 = SignedRoster::sign(&b2, &owner).unwrap();
        verify_next(&g, &gb, &s2).unwrap();
        // The member cannot sign the next version.
        let mut b3 = b2.successor(&s2, 3);
        let e = b3
            .devices
            .iter()
            .position(|e| e.device.id == phone.id())
            .unwrap();
        let removed = b3.devices.remove(e);
        b3.removed.push(RemovedEntry {
            entry: removed,
            removed_in: 3,
            last_seq: 7,
            last_hash: ZERO_HASH.into(),
        });
        b3.epoch = 1;
        let by_member = SignedRoster::sign(&b3, &phone).unwrap();
        assert!(verify_next(&s2, &b2, &by_member).is_err());
        let s3 = SignedRoster::sign(&b3, &owner).unwrap();
        verify_next(&s2, &b2, &s3).unwrap();
        // Removal without a new epoch is refused.
        let mut b3x = b3.clone();
        b3x.epoch = 0;
        let s3x = SignedRoster::sign(&b3x, &owner).unwrap();
        assert!(verify_next(&s2, &b2, &s3x).is_err());
        let bodies = verify_chain(&[g.clone(), s2.clone(), s3.clone()]).unwrap();
        assert_eq!(bodies.len(), 3);
        assert_eq!(bodies[2].genesis, g.hash());
        // A rolled-back or reordered chain fails.
        assert!(verify_chain(&[g.clone(), s3.clone()]).is_err());
        // Re-adding a removed device is refused.
        let mut b4 = b3.successor(&s3, 4);
        b4.devices.push(entry(&phone, DeviceRole::Member, 4));
        b4.sort();
        let s4 = SignedRoster::sign(&b4, &owner).unwrap();
        assert!(verify_next(&s3, &b3, &s4).is_err());
    }

    #[test]
    fn removing_an_owner_needs_the_recovery_key_and_recovery_only_adds() {
        let owner = DeviceSecret::generate().unwrap();
        let other = DeviceSecret::generate().unwrap();
        let rec = DeviceSecret::generate().unwrap();
        let space = SpaceId::random().unwrap();
        let mut gb = RosterBody {
            v: 1,
            space,
            version: 1,
            epoch: 0,
            prev: ZERO_HASH.into(),
            genesis: String::new(),
            devices: vec![
                entry(&owner, DeviceRole::Owner, 1),
                entry(&other, DeviceRole::Owner, 1),
                entry(&rec, DeviceRole::Recovery, 1),
            ],
            removed: vec![],
            made_ms: 1,
            history_batches: 0,
            history_head: String::new(),
        };
        gb.sort();
        let g = SignedRoster::sign(&gb, &owner).unwrap();
        let mut b2 = gb.successor(&g, 2);
        let i = b2
            .devices
            .iter()
            .position(|e| e.device.id == other.id())
            .unwrap();
        let e = b2.devices.remove(i);
        b2.removed.push(RemovedEntry {
            entry: e,
            removed_in: 2,
            last_seq: 0,
            last_hash: ZERO_HASH.into(),
        });
        b2.epoch = 1;
        // A stolen phone alone cannot remove the other owner device.
        let mut s2 = SignedRoster::sign(&b2, &owner).unwrap();
        assert!(verify_next(&g, &gb, &s2).is_err());
        // With the recovery key's second signature it can.
        s2.cosign(&rec);
        verify_next(&g, &gb, &s2).unwrap();
        // A wrong second signature fails.
        let mut s2x = SignedRoster::sign(&b2, &owner).unwrap();
        s2x.cosign(&other);
        assert!(verify_next(&g, &gb, &s2x).is_err());
        // The recovery identity itself may not remove anyone, even co-signed.
        let mut s2r = SignedRoster::sign(&b2, &rec).unwrap();
        s2r.cosign(&rec);
        assert!(verify_next(&g, &gb, &s2r).is_err());
        // But it may add a device (a restore).
        let fresh = DeviceSecret::generate().unwrap();
        let mut b3 = gb.successor(&g, 2);
        b3.devices.push(entry(&fresh, DeviceRole::Owner, 2));
        b3.sort();
        verify_next(&g, &gb, &SignedRoster::sign(&b3, &rec).unwrap()).unwrap();
    }

    #[test]
    fn a_server_cannot_insert_a_device() {
        let owner = DeviceSecret::generate().unwrap();
        let rec = DeviceSecret::generate().unwrap();
        let evil = DeviceSecret::generate().unwrap();
        let space = SpaceId::random().unwrap();
        let (g, gb) = genesis(&owner, &rec, space);
        let mut b2 = gb.successor(&g, 2);
        b2.devices.push(entry(&evil, DeviceRole::Owner, 2));
        b2.sort();
        // Signed by the intruder itself.
        let s2 = SignedRoster::sign(&b2, &evil).unwrap();
        assert!(verify_next(&g, &gb, &s2).is_err());
        // Or the body changed after the owner signed.
        let honest = SignedRoster::sign(&gb.successor(&g, 2), &owner).unwrap();
        let mut forged = honest.clone();
        forged.body = serde_json::to_vec(&b2).unwrap();
        assert!(verify_next(&g, &gb, &forged).is_err());
        // A different genesis (fake history) signed by the intruder.
        let (fake, _) = genesis(&evil, &rec, space);
        assert_ne!(fake.hash(), g.hash());
    }

    #[test]
    fn labels_are_sealed() {
        let c = KeyChain::generate().unwrap();
        let s = SpaceId::random().unwrap();
        let d = DeviceSecret::generate().unwrap().id();
        let l = seal_label(&c, s, d, "Sam's phone").unwrap();
        assert_eq!(open_label(&c, s, d, &l).unwrap(), "Sam's phone");
        let other = DeviceSecret::generate().unwrap().id();
        assert!(open_label(&c, s, other, &l).is_err());
    }
}
