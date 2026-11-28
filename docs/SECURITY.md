# Security: sync and encryption

Status: Phase 6, 2026-11-28; revised the same day after the independent
review ([security-reviews/2026-11-28-phase6-review.md](security-reviews/2026-11-28-phase6-review.md)). Written for the owner first (plain language),
then for a reviewer (exact constructions and byte layouts). Decisions are
D-89 to D-98 in [DECISIONS.md](DECISIONS.md). Code: `crates/crypto` (every
primitive and format), `crates/sync` (the client), `crates/server` (the
relay).

## In plain language

**Sync is off unless you turn it on.** Without it, nothing leaves the device
(D-11). With it, your devices share the household through a server you
choose (for now, one you or someone you trust runs).

**The server cannot read your household.** Every change and every photo is
locked on your device before it is sent, with a key the server never has.
The server stores locked boxes and passes them on. That holds even if the
server is hacked, or its operator is curious, or a court asks them: they
have nothing readable to give.

**What the server operator can see**, because no design can hide it from
the machine that stores and relays the boxes:

- that a household exists, how many devices it has, and their public keys
  (each device is identified by a key, not a name or e-mail);
- which devices belong to the same person (an anonymous person number, not
  a name) and each device's role (owner, member, limited);
- when each device connects, roughly how much it sends and how often
  (number of changes in a batch is hidden; sizes are rounded up to 256
  bytes for changes and 4 KB for files);
- how many photos and documents there are and their approximate sizes;
- the internet address devices connect from (as any server would);
- when a device is added or removed, and the time a batch's newest change
  was made (used to stop wrong clocks; see below).

**What the operator cannot see:** task names, things, rooms, notes,
completions, costs, the home facts sheet and its codes, photos, documents,
people's and devices' names, the household's name, the address.

**Adding a device or person.** An owner's device shows a code (and a QR
code). The new device types it. The code is good once, for ten minutes, and
is not the household's key: it only lets the two devices talk privately
through the server. Both screens then show the same six-digit number. The
new device's person says "they match" first; only then can the owner say
yes. If someone were interfering, the numbers would differ and nothing
would be shared, provided the people actually compare them: the owner's
screen says to say no if the new device shows an error or another number.

**Removing a device or person.** An owner removes it from the device list.
The server stops serving it at once, and the household gets a new key, so
the removed device cannot read anything added afterwards, and cannot write
anything new: every device refuses its batches except the ones it really
sent before removal. Two honest limits: what was already on that device
stays on it (removing cannot reach into a phone and delete data); and if
the person removed also controls the sync server, the server could keep
the removal from your other devices for a while. Devices notice when
another device knows of a newer device list than the server gives them,
and then stop sending changes until it arrives (D-103, D-106; only a
version signed by an owner's device counts, so a member cannot fake one),
but a server that hides it from every device at once is not detected. Removing an owner's
device needs the recovery key (D-101). Every device shows a notice when a
device is added or removed elsewhere.

**The recovery key** is shown once when sync is set up, and can be printed
on the sealed volume of the house binder. It is the only way to get the
household back if every device is lost. Housewright does not keep a copy,
on the device or the server. **If every device and the recovery key are
lost, the data is gone; nobody can recover it.** It is also the
household's master key: whoever holds it can read everything, add a device
and, with that device, remove the others. If the paper is seen by the wrong
person, make a new recovery key at once (Settings, Sync and devices; this
needs the current key), check the device list and remove anything unknown.
If the paper is lost (not stolen), the owner cannot make a new one without
it: make a full backup, stop syncing, and set sync up again (a new
household on the server; other devices pair again).

**A stolen phone.** If it is unlocked (or its lock is weak), the thief can
see everything on it, as with any app: the household's data is stored on
the device so the app works offline. Remove that phone from another
device's device list as soon as you can (this needs the recovery key if
it was an owner's phone): it then gets nothing new. A stolen owner's phone
can add devices and remove members' devices, but it cannot remove the
owner's other devices or replace the recovery key without the paper; every
device is told about what it does. Use a screen lock. (Encrypting the database on the phone would not help against a
thief who can unlock the phone, because the app has to be able to open it.)

**Wrong clocks.** Changes carry the time they were made. A device whose
clock is far ahead could otherwise make its edits win over everyone's for
years. So the server and every device refuse changes stamped more than five
minutes ahead of the server's clock, and the app says "this device's clock
looks wrong" in plain words. The changes wait on that device until its
clock is right.

## Threat model

| Who | Can | Cannot (by design) |
|---|---|---|
| Server operator, or anyone who copies the server's disk | see the metadata above; delete or withhold data, including hiding a new device list from every device at once (denial of service, and delayed removals); replay old data it holds; make chosen devices refuse a batch by lying about receive times | read any household content; change content undetected; add a device to a household; impersonate a device; make devices accept an operation from outside the household, or a second batch under a device's used number; move data between households or key epochs |
| Network attacker (no TLS, or broken TLS) | the same as the operator, plus see who talks to which server | the same as the operator: TLS protects metadata only, the content is end-to-end encrypted anyway |
| Someone who sees the pairing code (a photo of the QR) | try to join in the ten minutes before the real device | join unnoticed: the safety number would not match (see "Pairing") |
| A removed member | keep what was already on their device; with the help of the server operator, read what other devices still seal under the old key while the server hides the removal from all of them (D-103) | read anything sealed after the other devices know of the removal; push or pull through the server; inject batches through a colluding server (refused by every device, D-99) |
| A household member | read everything (they hold the household key); write changes in their own name, including edits and deletes of shared things (a delete is permanent on every device; only a backup brings it back) | add or remove devices; write changes, or records ("done by"), in another member's name; replace another member's records (all refused by every device, D-104) |
| A thief with an unlocked device | read everything on it; act as that device until it is removed; if an owner's: add devices, remove members' devices (everyone is told) | remove the owner's other devices or replace the recovery key without the paper (D-101); anything after removal |
| A thief with the recovery key paper and the server address | read everything; add a device as owner; with that device and the paper, remove the owner's devices and replace the key, i.e. take the household over | do it unseen: every remaining device is told; nothing, if the owner replaced the key first |

Out of scope for Phase 6: a compromised device operating system, attacks
on the app's update channel, side channels on the device, and a server that
equivocates (shows different devices different histories; see "Known
weaknesses").

## Design for a reviewer

All primitives come from mainstream RustCrypto and dalek crates; no
primitive is home-made. Versions as locked on 2026-11-28:

| Crate | Version | Used for |
|---|---|---|
| `ed25519-dalek` | 3.0.0 | Ed25519 signatures (`verify_strict` everywhere) |
| `hpke` (rust-hpke) | 0.15.2 | RFC 9180 HPKE, base mode, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305 |
| `x25519-dalek` / `curve25519-dalek` | 3.0.0 / 5.0.0 | inside `hpke` and `ed25519-dalek` |
| `chacha20poly1305` | 0.11.0 | XChaCha20-Poly1305 |
| `hkdf` | 0.13.0 | HKDF-SHA256 |
| `hmac` | 0.13.0 | HMAC-SHA256 |
| `sha2` | 0.11.2 | SHA-256 |
| `zeroize` | 1.9 | wiping secrets from memory |
| `getrandom` | 0.4.3 | the operating system's random number generator (Web Crypto `getRandomValues` in the browser) |

Earlier versions of the dalek crates and of `chacha20poly1305` were audited
(Quarkslab 2019 for curve25519-dalek/ed25519-dalek; NCC Group 2020 for the
RustCrypto AEADs); the exact versions above have not been separately
audited, and `hpke` has had no published audit (it is tested against the
RFC 9180 vectors). A reviewer should treat "audited" as "audited family".

### Encodings used everywhere

- `transcript(label, parts…)` = `u32be(len(label)) ‖ label ‖ u32be(len(p1)) ‖ p1 ‖ …`.
  Every signature input and every AEAD associated data is a transcript with
  its own label (`housewright/v1/...`), so no two contexts can collide.
- Binary values in JSON are base64url without padding; ids are lowercase hex.
- Signatures are over exact bytes (the roster body is signed as the bytes
  sent, not a re-serialisation).

### Identities and keys

- **Device**: two 32-byte random seeds. Ed25519 signing key from the first;
  X25519 key pair from the second through HPKE `DeriveKeyPair`. Device id =
  first 16 bytes of `SHA-256(transcript("housewright/v1/device-id", ed25519_pk))`,
  so an id is bound to its signing key (`DevicePublic::check`).
- **Space**: the household's id on the server, 16 random bytes, unrelated to
  the household id inside the data (a UUIDv7, which embeds a timestamp).
- **Household key chain**: `K_0, K_1, …`, 32 random bytes each, one per
  epoch. Serialised as `u32be(n) ‖ K_0 ‖ … ‖ K_{n-1}`.
- **Purpose keys**: `HKDF-SHA256(ikm = K_e, salt = space, info = label ‖ u32be(e))`
  with labels `housewright/v1/key/ops`, `…/blobs`, `…/labels`,
  `…/blob-names` (the last always from `K_0`).

### Key envelopes (wrapping the chain to a device)

HPKE base mode, single-shot, suite X25519-HKDF-SHA256 / HKDF-SHA256 /
ChaCha20-Poly1305:

- `info = transcript("housewright/v1/wrap-info", space, recipient_id, recipient_x25519_pk, u32be(epoch))`
- `aad = transcript("housewright/v1/wrap-aad", sender_id)`
- plaintext = the whole key chain up to `epoch`.
- The sender signs `transcript("housewright/v1/wrap-sig", space, recipient_id, u32be(epoch), sender_id, enc, ct)`.

A receiver opens an envelope only if the signer is an owner (or the
recovery identity) in the roster version that started that epoch (or a
manager added later in the same epoch), and only if the new chain extends
the chain it already has (`is_prefix_of`).

The baseline asked for XChaCha20-Poly1305 here; HPKE uses ChaCha20-Poly1305
with nonces derived inside HPKE, which is safe (one message per context) and
lets the construction be the standard one with test vectors rather than our
own composition (D-89).

### Operation batches

A device pushes its operations in batches (up to 400 operations or about
512 KB). Header, in the clear:

```
v=1, space, epoch, batch_id (16 random bytes, hex), device, device_seq (u64,
the device's own counter), max_wall_ms (largest HLC wall time inside),
nonce (24 random bytes)
```

- `aad = transcript("housewright/v1/batch", v, space, u32be(epoch), batch_id, device, u64be(device_seq), u64be(max_wall_ms), nonce)`
- `ct = XChaCha20-Poly1305(ops key of epoch, nonce, pad(JSON ops, 256), aad)`;
  `pad` = `u32be(len) ‖ data ‖ zeros` to a multiple of 256 bytes.
- `sig = Ed25519(device, transcript("housewright/v1/batch-sig", aad, ct))`.

The server checks the signature with the device's registered key, that the
header names the authenticated device and its household, that the epoch is
not ahead of the household's, and the clock guard. Every receiver checks,
in order (`SyncClient::accept`):

1. the author device is in the current roster, or was removed with
   `device_seq ≤ last_seq` (the remover's last-seen number for it);
2. it is not the recovery identity;
3. signature, then decryption under the epoch's key with the header as AAD
   (so a batch cannot be moved to another household, epoch, device or
   sequence number; it would fail to decrypt even if an insider re-signed it);
4. every operation's clock ≤ `max_wall_ms` ≤ server receive time + 5 min;
5. every operation's `author`, and the `by` of every record, is the member
   the device belongs to (owners included); the only exception is history
   from before sync was set up (operations older than the genesis roster),
   sent once by the founding device.

Before those content checks, the chain rules (D-99, D-107): each device's
batches form a hash chain (`prev` in the signed header and the AAD). A
receiver applies a device's batch only as the next link after the last one
it applied from it. A batch further ahead is held (signature checked, one
per number) while the missing numbers are fetched by device and number; a
second, different batch under a number already used is refused (counted as
tampering). A removed device's batches are held (signed by it, at most four
candidates per number, within its recorded count) until the chain ending
at the `last_hash` recorded in the roster is present, then applied in
order; anything else from it is refused.

The founding device's exception to rule 5 covers exactly the batches the
genesis roster commits to (their number and the last one's hash, D-110),
sealed at setup.

Each batch also carries its author's last eight signed roster versions
(D-106, D-112). A receiver believes a newer version only if those link to
its own verified chain, step by step, under the same rules as any roster
update. A claim not signed by an owner's device is counted as that author's
tampering and changes nothing. A verified version the server will not give
the device, or a verified fork (a different valid successor under a number
the device has), pauses its sending, with a notice naming the device that
made the claim; the person can press OK to send anyway. Each device can
cause such a pause only once; after OK, its later claims only add a notice
(D-115).

A batch that fails is skipped by every device the same way as long as the
server shows every device the same batches in the same order (an honest
server does; a server that reorders or equivocates can make devices
disagree, see "Known weaknesses"). An authentic batch this version cannot
read (from a newer app) is kept and retried after an update, never
dropped.
Operations then go through `Store::apply_synced_ops`, which repeats the
clock guard per operation and records them as received (never pushed back).

Retries: a batch is sealed and saved before it is sent; a retry after a
crash or dropped connection sends the same bytes; the server deduplicates by
`batch_id` and refuses a different batch reusing a `(device, device_seq)`.

### Files (blobs)

- Server name: `hex(HMAC-SHA256(blob-name key, sha256_hex)[..16])`, from
  epoch 0 so a file keeps one name. The server cannot test "is this known
  file stored here?" without the key.
- Content: `"HWB1" ‖ u32be(epoch) ‖ nonce(24) ‖ XChaCha20-Poly1305(blob key of epoch, nonce, pad(bytes, 4096), aad)`,
  `aad = transcript("housewright/v1/blob", space, name, u32be(epoch))`.
- After decrypting, the receiver checks SHA-256 of the plaintext against
  the hash in the attachment record (signed, encrypted, written once and
  bound to its author, D-104), so a file cannot be swapped by the server.
  A member can still attach a different file of their own; they cannot
  change someone else's attachment.
- Nothing is deleted or overwritten because of what a device reports
  (D-108, D-113). The server keeps one copy per uploading device; no
  device can replace another's. A receiver tries every copy against the
  hash in the signed record. A device missing the key for a file's epoch
  fetches the keys and tries again. If no copy matches, the name is marked
  and a device holding the real file adds its own copy.
- Photos' thumbnails and files up to 4 MB are fetched during sync; larger
  ones when opened.
- Files on the device stay as they were in Phase 4 (plain files in app
  storage, like the database); see D-95.

### The roster (who is in the household)

The server must not decide membership, or it could add its own key and be
sent the household key at the next rotation. The roster is a hash chain of
signed versions (`crates/crypto/src/roster.rs`):

- body (JSON, signed as sent): `v, space, version, epoch, prev (hash of the
  previous signed version), genesis (hash of version 1), devices[], removed[], made_ms`.
- each entry: device public keys and id, member id, role (owner, member,
  limited, recovery), the version it was added in, and its name sealed with
  `XChaCha20-Poly1305(labels key of the epoch it was added in, aad = transcript("housewright/v1/label", device_id))`.
- signature: `Ed25519(signer, transcript("housewright/v1/roster-sig", body))`;
  hash: `SHA-256(transcript("housewright/v1/roster-hash", body, signer, sig))`.
- rules (`verify_genesis`, `verify_next`): version 1 is signed by its owner
  device, epoch 0, exactly one recovery identity; each later version is
  version + 1, links `prev` and `genesis`, is signed by an owner or the
  recovery identity listed in the previous version; entries never change;
  removals are permanent and carry `last_seq`; a removed id can never be
  re-added; any removal forces `epoch + 1`; there is always at least one
  owner device and exactly one recovery identity.

Every client verifies the whole chain when it loads its state and every new
version when it arrives; the server verifies too, but nobody relies on it.
Trust anchors: a paired device requires that the version adding it was
signed by the device whose key arrived in the sealed pairing offer, and that
the chain's genesis hash equals the one in the offer; a restoring device
requires the genesis hash in the anchor sealed with the recovery key.

### Key rotation (removing a device, replacing the recovery key)

An owner device syncs first (to learn the removed device's last batch
number), makes `K_{e+1}`, writes a roster version with the device moved to
`removed` and `epoch = e + 1`, wraps the extended chain to every remaining
device and the recovery identity, and posts it; the server applies it in one
transaction (revokes the device, stores the envelopes). Old epochs stay
readable to everyone who remains because the chain is wrapped whole.

Replacing the recovery key is the same with the old recovery identity
removed and a new one added, plus a new anchor.

### Signed requests

Every request from a device carries `hw-device`, `hw-time` (ms),
`hw-nonce` (16 random bytes) and
`hw-sig = Ed25519(device, transcript("housewright/v1/request", METHOD, path-and-query, u64be(time), nonce, SHA-256(body)))`.
The server refuses requests more than 5 minutes from its clock (answering
with its time, so an honest client can correct its request times), and
remembers each `(device, nonce)` for ten minutes in SQLite (so a restart
does not reopen the window). Chosen over challenge-response sessions
(D-90): nothing secret is issued, nothing can leak from logs or proxies, and
each request is bound to its exact body. The path signed is relative to the
server's base path, so a reverse proxy may strip a prefix.

Unauthenticated endpoints (reading, answering and aborting an invitation,
`/v1/health`) and creating a household are rate limited per client address
(token buckets in memory: 30 per minute for pairing, 5 per hour for new
households, 10 per minute for recovery lookups). Bodies are limited to 4 MB
(26 MB for files). CORS allows any origin: there are no cookies or ambient
credentials to protect.

### Pairing (D-94)

1. The owner device makes a code: 40-bit invitation id + 80-bit secret,
   24 Base32 characters; and a random `r` (32 bytes). It posts the offer
   `{space, its public keys, role, member id, genesis hash, roster version,
   commit = SHA-256(transcript("housewright/v1/pair-commit", r)), household
   name, expiry}` sealed with
   `k = HKDF-SHA256(ikm = secret, salt = invitation id, info = "housewright/v1/pairing")`,
   XChaCha20-Poly1305, `aad = transcript("housewright/v1/pair-offer", invitation id)`.
2. The new device opens the offer (a wrong code fails to decrypt), makes its
   keys and posts its public keys and name sealed the same way (`pair-request`).
   The server accepts one answer per invitation.
3. The owner device opens the answer and posts `r` sealed (`pair-reveal`).
4. The new device checks `r` against the commitment. Both show
   `safety number = SHA-256(transcript("housewright/v1/safety-number", invitation id, r, owner keys, new keys))`
   reduced to six digits.
5. The person confirms on both. The owner device posts a roster version
   adding the device (signed) and an envelope; the new device verifies as
   described and syncs.

The server never sees the secret, so it cannot read or alter the exchange.
If the code leaks (say, a photographed QR), a middleman must fix its own
keys toward the owner device before `r` is revealed, so it cannot search
for keys giving matching numbers; the chance the numbers match by luck is
one in a million. The test `a_middleman_with_the_code_is_caught_by_the_safety_number`
plays this attack against the real server code.

### The recovery key (D-91)

160 random bits as 32 Base32 characters plus a 20-bit checksum, nine groups
of four. Never stored after it is shown. From it, `DeviceSecret::derive`
(HKDF-SHA256, salt `housewright/v1/derived-device`) gives the **recovery
identity**, a roster entry with role `recovery` that receives envelopes and
may sign roster versions; and an **anchor key**
(`HKDF(R, "housewright/v1/recovery", "anchor")`) that seals the genesis
hash, stored on the server. Restoring: derive, look up the household
(`GET /v1/recovery`, signed by the recovery identity), open the anchor,
verify the chain from that genesis, open the newest envelope, add a fresh
owner device signed by the recovery identity, sync.

160 bits is beyond offline search, so no password stretching is needed.

### The clock guard (D-93)

Phase 1 left remote clocks unchecked: one device with a clock years ahead
would push every device's hybrid logical clock into the future. Now the
reference is the **server's** receive time, the same for every receiver:
`apply_synced_ops(ops, received_ms)` refuses an operation whose wall time is
more than 5 minutes past it; batches also carry `max_wall_ms`, which the
server checks on push (`clock_ahead`, so the sending device can tell the
person) and receivers check again. The receiver's own clock is deliberately
not the reference: two receivers with different clocks would then accept
different operations and stop converging. A device's own clock error is
measured from the server's `hw-server-time` header and shown as a plain
warning when it is over five minutes.

### Keys at rest on the device (D-95)

The device secret and the key chain are stored in the app's database
(`sync_state` table), in the app's private storage: Android's app data
folder, the browser's origin-private file system. The household's data sits
in the same database unencrypted (as since Phase 1), so wrapping the keys
alone would not protect the data from someone who can read the folder.
**TODO:** wrap the device secret with an Android Keystore key (and on the
web, a non-extractable WebCrypto key in IndexedDB). On the web this gives
little: whoever can run script in the page's origin can use the key.
Secrets are wiped from memory (`zeroize`) when dropped; JSON and HTTP
buffers holding plaintext operations are not all wiped.

### What the tests prove (`crates/sync/tests`, `crates/api/tests/sync.rs`, `app/tests/sync.spec.ts`)

- three devices converge under random interleavings of edits, deletes,
  syncs, offline periods, dropped connections and server restarts (property
  test, `convergence.rs`);
- after seeding distinctive titles, a sensitive code, a photo and device
  names, no byte of the server's SQLite file, WAL or blob folder contains
  them (`server_holds_no_plaintext`, and again in the browser test);
- a flipped ciphertext bit, a forged signature and a batch copied from
  another household are refused by receivers; the server refuses tampered
  batches and replayed or stale requests;
- a removed device is refused by the server and cannot open new-epoch
  batches with the keys it kept;
- the recovery key restores everything (data and files) on a fresh device,
  a typo is caught, a replaced key stops working;
- the clock guard on push, on receive, and against a server lying about
  receive times;
- pairing with a wrong code, with "they don't match" on either side, and
  with a middleman who has the code;
- a member device writing in the owner's name is refused.
- after the third verification pass (also `verify2.rs`): a stolen owner
  phone's repeated fork claims pause sending at most once (Q1); growing
  one's own file copy past the quota is refused (Q2); `restore` cannot
  skip the removal race check (`restore_flag_does_not_reopen_the_removal_race`).
- after the second verification pass (`crates/sync/tests/verify2.rs`): a
  stolen owner phone's bogus newer claim freezes nobody, and a real but
  unpublished one pauses sending until OK (P1); a member cannot replace a
  file, and the original copy still opens (P2); a server restored from
  before a removal converges and revokes the removed device again (P3);
  forks filling a removed device's slots cannot block its real batches
  (L-a). Each protection was switched off to confirm its test fails.
- after the verification pass (`crates/sync/tests/verify.rs`): a member's
  made-up roster claims freeze nobody (N1) and a real hidden version clears
  when it arrives; one lost batch is sent again and the gap filled (N2); a
  server restored from an old backup recovers, and a new device can still
  join (N2); a device with old keys never loses a good file, and a member
  marking a good file bad loses nothing (N3); one address cannot use up
  everyone's pairing allowance (N4, server unit test); unsigned batches for
  a removed device are not kept (N5); the founder cannot backdate changes
  in another member's name (N6); a constant "numbers match" is refused
  (L6). Each protection was switched off in turn to confirm its test fails.
- after the review (`crates/sync/tests/review.rs`): a removed device with a
  colluding server injecting forged batches, including to a device that
  joins later and is shown the forgeries first (H1); an owner's device
  writing in a member's name (H1); 14 MB of big batches through a phone
  that cannot read answers over 2 MB, a change too big to send, a stored
  batch too big to fetch, a batch over the server's cap (H2); a forged
  "done by" and a backdated rewrite of someone's record (M2); a stolen
  owner's phone trying to remove the owner's devices or replace the key,
  and notices on other devices (M3); a roster version hidden by the server
  stopping sealing (M1); the household quota (M4); a squatted file healing
  (L2); an unreadable batch kept (L3); a batch re-sealed after a removal
  (L4); the owner unable to finish pairing first (L6); "stop syncing"
  leaving no key in the database file or its log (L7). Each of the H1, H2,
  M1 and M2 protections was switched off in turn to confirm its test then
  fails.

### Size limits (D-100)

The server refuses a batch over 1 MiB of ciphertext; clients keep batches
under 512 KiB and keep a single change too large to send on the device
(said plainly). Pull pages stop at 8 MB; a client that cannot read a page
asks for fewer batches, down to one; a stored batch over the limit is
skipped and counted, never fetched forever. Each household has a storage
quota (2 GB by default) and, by default, a registration token is needed to
create a household (D-105).

### Known weaknesses and open questions

- **Equivocation and rollback.** A malicious server can withhold the
  newest roster version or hold back batches. Each batch now carries its
  author's newest roster version and hash inside the encryption (D-103):
  a device that sees a version it cannot get stops sealing, and a
  different hash for a version it has is flagged as a fork. What remains:
  a server that hides a version from every device at once (including from
  the device that made it, after it was made) is not detected, and a
  restoring device trusts the newest roster the server shows it (it is
  warned only if a later batch names a newer one).
- **After "OK", a signer's claims only raise notices.** To stop a stolen
  owner phone pausing sync forever, each device pauses sending at most once
  per signing device (D-115). After the person presses OK, a later roster
  change that this signer reports but the server hides is shown only as a
  notice, not a pause. Take those notices seriously: check the device list
  on an owner's device. A stolen owner phone can still add new owner
  devices (each announced on every device), and each of those can pause
  sending once.
- **A failed sync during pairing drops the invitation.** If the sync the
  owner's device runs while confirming a new device fails, start pairing
  again.
- **No forward secrecy within an epoch.** Keys change only when someone is
  removed. Whoever compromises a current device gets every epoch's key and
  can read the whole history, including old files the server still holds.
- **Recovery key = master key** (see the threat table). The cosignature
  rule (D-101) stops a stolen phone; it cannot stop someone who has the
  paper. Notices on every device make a takeover visible, not impossible.
- **Deletes are permanent.** Any member's delete wins over every edit on
  every device; only a backup brings the thing back.
- **Record conflicts are first-applied wins** (D-104). Honest devices
  never collide (record ids are random); an attacker who could guess an id
  before it was sent, or a server that reorders batches, could make devices
  disagree about one record. A file import replays history in clock order
  and could choose differently in that (attack-only) case.
- **Withheld batches**: a gap in a device's chain is refused (and counted),
  not yet explained to the person.
- **Metadata** listed above is visible to the server, including the time of
  each batch's newest change (`max_wall_ms`).
- **Limited members are limited only in the interface** (D-98): their
  devices hold the full household key.
- **Members can rewrite shared data** (that is what members are for), and a
  member's device could write a role change into the household data; roles
  in the data are not cryptographically enforced. Only key distribution is.
- **A device whose clock was far ahead** when changes were made keeps those
  changes until real time catches up; there is no tool yet to re-stamp them.
- **The safety number** is only as good as the person's comparison; the
  interface asks plainly and both sides must confirm.
- **The PDF with the recovery key**, printed at setup, passes through the
  platform's viewer (Android keeps it in the app's cache until the next
  shared file replaces it; the browser in a blob URL until the tab closes).
- **Dependency advisories**: `cargo deny check` passes except two
  denial-of-service advisories in quick-xml 0.38 (RUSTSEC-2026-0194, -0195),
  reached only through Typst's bibliography code, which the binder never
  uses on outside input; ignored with that reason in `deny.toml` until
  Typst upgrades.
- **Server robustness**: SQLite sits behind one lock; heavy handlers run
  on blocking threads and files are streamed, but a very busy server would
  want a connection pool.
- **Server restored from an old backup** (D-107, D-114), including one
  taken before a removal: devices notice (the
  server's newest batch number is below theirs), read everything again,
  and send back what it lost: each device its own batches (it keeps every
  batch it sealed) and an owner's device the roster versions, with fresh
  key envelopes. A lost change of recovery key cannot be sent back without
  the paper; the app asks for a new recovery key then. Changes made by a
  device that was itself lost (or removed) since the backup are gone from
  the server, as they would be anyway; devices that already had them keep
  them. Removed devices are revoked again when an owner's device repairs
  the server; until then a member's device keeps its changes and says it
  is waiting for an owner's device.
- **Households set up before commit 3d058c3** (test households only;
  nothing has shipped) have no history commitment in their genesis roster:
  devices joining them now refuse the founder's pre-sync history. Set such
  households up again.
- **A server can hide a removal from one device** if it also hides the
  remover's later batches from that device. That device then shows
  "Some changes from … are missing": act on it (open the other device, look
  at the device list on an owner's device, check who runs the server).
  Until it clears, that device may keep sealing under the old key.
- **Pairing rests on comparing the two screens.** The confirmation is bound
  to the safety number's inputs (D-111), but someone who has the code and
  joins first computes it for their own keys; only the person looking at
  both screens stops that. The app says so on both screens.
- **A removed device and a hostile server can still withhold** that
  device's real pre-removal batches from a new device (they can always
  withhold); they cannot block them by flooding forks (the walk back from
  the recorded hash fetches the needed batch by number).
- **Storage on devices**: each device keeps every batch it sealed (to send
  again after a server restore), and one small record per batch it applied.
