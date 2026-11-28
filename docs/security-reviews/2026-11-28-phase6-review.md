# Housewright Phase 6 sync: independent security review

Branch `phase-6-sync`, worktree `/home/hoid/Desktop/hw-sync`, HEAD `b0b650f`. Reviewed 2026-11-28.
Scope: `crates/crypto`, `crates/sync`, `crates/server`, the sync paths of `crates/core` (oplog, hlc, store apply, sync_state), `crates/api/src/sync.rs`, the web XHR transport (`web/src/wasm.rs`, `app/src/lib/api/worker.ts`), `docs/SECURITY.md`, `docs/SELF-HOSTING.md`, D-89..D-98.
Method: I read the code first, then the docs. I ran `cargo test -p housewright-crypto` (22 pass) and two small programs in the scratchpad: `forge/` (finding H1) and `urlcheck/` (finding L1). Nothing in the worktree was changed.

## Verdict for the owner (plain language)

**Ship after fixes.**

The encryption is well built. It uses standard, well-regarded building blocks and puts them together correctly. A server operator who only reads what is stored cannot read your household's data. That is the main promise (D-14), and it holds.

Two problems need fixing before people rely on sync:

1. **Removing someone does not fully cut them off if they also control the server.** Picture a self-hosted home server run by a person who is later removed from the household, such as an ex-partner. That person can keep slipping new changes into everyone's household, signed as their old phone, and every device accepts them. If the removed phone belonged to an owner, those changes can be made in anyone's name. (H1)
2. **One member can make sync stop working for every device, permanently.** This needs no special access, and a very large household could trigger it by accident. Fixing it would take someone editing the server's database by hand. (H2)

Some plain-language promises in SECURITY.md also claim more than the code delivers:

- "A file cannot be swapped even by a member": not true (M2).
- "Members cannot write in another member's name": only partly true (M2).
- "A thief with the recovery key cannot do anything after you make a new key": a thief with the key can lock you out before you get the chance (M3).
- "`--max-households` stops strangers filling your disk": it does not (M4).

These need either a code fix or an honest rewrite.

## Findings

### High

#### H1. A removed device, working with the server, can inject forged changes forever. **Confirmed** (demonstrated)

- Where:
  - `crates/sync/src/client.rs:967-975` (`find_author`)
  - `client.rs:516-555` (`accept`)
  - `crates/crypto/src/roster.rs:84-89` (`RemovedEntry.last_seq`)
- **What is wrong.** A removed device still counts as a valid author for any batch whose `device_seq <= last_seq`. Nothing checks that such a batch is one the device actually sent before it was removed.
  - The removed device still holds its Ed25519 key and every key up to the epoch it was removed in. Receivers accept batches in any old epoch.
  - The only rule against reusing a `(device, device_seq)` lives on the server (`handlers.rs:471-480`, `UNIQUE (space, device, device_seq)`). Receivers keep `seen` but never use it to refuse anything.
  - A removed entry keeps its old role. If the removed device was an owner's, `accept` skips the author-name check (`client.rs:549`), so the forged operations can carry any member's name.
- **Attack.** An ex-member runs (or later gains control of) the server.
  1. After removal, they seal a new batch: their old device id, `device_seq = 1..last_seq`, epoch 0 key, any operations they like (deletes, edits, records in anyone's name).
  2. The server inserts it with `received_ms` set to now.
  3. Every remaining device accepts and applies it. The batch passes signature, decryption, author, clock and authorship checks.
  4. Tombstones win over every edit, so injected deletes are permanent.
- **Demonstration.** `scratchpad/forge/src/main.rs` builds a real two-version roster (owner removes a device with `last_seq = 5`, epoch moves to 1). The removed device seals a new batch at seq 3 under epoch 0. The copied `find_author` plus the real `open_batch` accept it. Output: `author role = Owner`, `receiver accepted forged post-removal batch`.
- **Gap with the docs.** SECURITY.md says the operator cannot "make devices accept an operation from outside the household", and that a removed member cannot "push or pull through the server". This is not the equivocation case the docs mark out of scope: every device sees the same forged batch.
- **Fix** (any one closes it):
  - **(a)** Chain each device's batches: add a new `prev_batch_hash` field to the signed header and to the batch AAD. Record the head hash at `last_seq` in `RemovedEntry` (`last_hash`). Receivers accept a removed device's batch only if it is on the chain that ends at `last_hash`.
  - **(b)** The remover records a hash (or Merkle root) of the batch ids it accepted from the removed device. Receivers accept only batches in that set.
  - Either way, receivers should also refuse a second, different batch for a `(device, seq)` they have already seen, and count it as tampering.

#### H2. One page of pulled batches can exceed the client's 64 MB response limit and stop sync for good. **Confirmed** (traced)

- Where:
  - Client asks for pages of 200 batches: `client.rs:487`
  - Native transport refuses bodies over 64 MB: `crates/sync/src/native.rs:411`
  - Server limits each push body to 4 MB (`crates/server/src/lib.rs:48`), with no per-batch size cap or per-page byte budget: `handlers.rs:409-449`, `522-549`
- **What is wrong.** Twenty batches of about 3.5 MB each make one page bigger than 64 MB once base64 and JSON are added.
  - `read_to_vec` fails, which becomes `SyncError::Offline`.
  - `st.cursor` only moves after a page succeeds, so every later sync asks for the same page and fails the same way.
  - The batches only need valid signatures: the server never decrypts, so their ciphertext can be random bytes.
  - The server side also builds each response fully in memory. A client asking `limit=500` over 4 MB batches makes the server allocate about 2.7 GB.
- **Attack.**
  1. Any member device (a modified app, or a Limited member's phone) pushes 20 or more large signed batches.
  2. Every Android or desktop device's sync is stuck permanently, and devices joining later never finish their first pull.
  3. The only fix is deleting rows from the server database by hand.
  4. Honest data could do the same. `BATCH_BYTES` is a soft 512 KB cap that one large operation can exceed, so 200 honest batches over about 230 KB each would overflow. A big imported household joined by a new phone is a realistic case.
- **Fix:**
  - **Server:** refuse any single batch larger than about 1 MB of ciphertext. Page by bytes as well as by count (stop at about 8 MB and set `more = true`).
  - **Client:** if a page is too large, halve `limit` and retry, down to 1.
  - **Client:** treat a batch that alone exceeds the cap as refused, skip past it with a recorded `rejected_batches`, and never stall.

### Medium

#### M1. A server can hide a removal (fork or withholding). A restoring device can be fed old history. **Confirmed** (traced). The weakness is documented, but the docs contradict themselves.

- Where:
  - `client.rs:383-406` (`refresh` accepts "no new versions")
  - `client.rs:849-905` (`restore` only checks that the genesis matches the anchor)
  - `crates/crypto/src/recovery.rs:221-226` (the anchor names only the genesis)
- **What is wrong.** Clients refuse rollback of a roster they already hold, but cannot tell when newer versions are being withheld.
  - A restoring device has no freshness check at all. The server can serve any verified prefix of the chain, plus the recovery identity's envelope from that prefix's epoch, and `restore` succeeds.
  - It then adds the new device under the old epoch, signs it, and posts it (a fork the server accepts).
  - A joining device is partly protected by `offer.roster_version` (`pairing.rs:531`).
- **Attack (removed member plus server).**
  1. The server hides roster version N (the removal) from the other devices, or serves the stale prefix to someone restoring.
  2. Those devices keep sealing new changes under epoch e, which the removed member can read.
  3. The restoring device even treats the removed device as a current member, because the stale prefix still lists it.
- **Docs.** "Known weaknesses" describes this honestly. But the plain-language section ("the removed device cannot read anything added afterwards"), and the threat-model row for a removed member, state it without the "only if the server is honest" condition. Self-hosted servers run by a household member make this collusion realistic.
- **Fix:**
  - Put the author's roster head hash (and epoch) inside each sealed batch, as the docs already propose. A device that sees a batch naming a roster version it cannot get, or a member still writing in an epoch that was rotated away, shows a fork warning.
  - For restore: owners can't re-seal the anchor without the paper. Instead, have each roster update wrap a "latest head" to the recovery identity (HPKE plus an owner signature), so a restore can detect a head older than one it was sent. That signal is weak, but it is better than none.
  - Fix the plain-language claims to say "provided the server delivers the update".

#### M2. Members can rewrite other people's permanent records and forge who did them. **Confirmed** (traced)

- Where:
  - `crates/core/src/store.rs:885-897` (`$record`: smallest order key wins)
  - `client.rs:548-553` (authorship check covers only `op.author`)
  - `crates/core/src/model.rs:440-454`, `586-592` (`Completion.by`, `Note.by`), `543-556` (`Attachment.sha256`)
- **What is wrong.**
  1. **Backdated overwrite.** An append-only record is meant to be written once. A conflicting `$record` for an existing id wins if its order key is smaller. The clock guard only limits clocks that are *ahead*, never *behind*. So a Member or Limited device can send `$record` for an existing completion, note or attachment id with `hlc.wall_ms = 0`, and it replaces the original on every device.
  2. **Forged attribution.** The `by` field inside a record is never compared with the authoring device's member. A Member device can append a completion or note with `by` set to the owner, and every device shows the owner as having done it.
- **Attack.** A member rewrites the owner's completion history (the service record you might show a buyer). Or they point an existing `Attachment` at a different file they uploaded (new `sha256`, new blob). The receiver's hash check passes, because it checks against the swapped record.
- **Docs.** This contradicts SECURITY.md:
  - lines 205-207: "a file cannot be swapped even by a member";
  - line 83: "write changes in another member's name (refused by every device)".
- **Fix:**
  - In `accept`, for every non-owner author, refuse a `$record` whose `by` (or other member-reference fields) is not `author.member`.
  - Make record conflicts impossible to win by forging. For example, require a `$record` to come from the same author as any existing record for that id; if the authors differ, choose deterministically in favour of the record whose `by` equals its `op.author`, then the smallest key. Or derive record ids from `op.id` and refuse any second write.
  - At minimum, correct the docs.

#### M3. A recovery-key holder, or a stolen unlocked owner phone, can take over the household for good. Docs understate this. **Confirmed** (traced)

- Where:
  - `crates/crypto/src/roster.rs:48-51` (Recovery may manage)
  - `client.rs:708-761` (remove), `807-846` (new recovery key)
  - `crates/api/src/sync.rs` (no re-authentication before these commands)
- **What is wrong.** Two takeover paths exist.
  - **The recovery identity** can sign a roster version that removes every real device and adds the attacker's own. No server collusion is needed: the server accepts it, because Recovery passes `need_manager`.
  - **A stolen owner phone** can remove every other device and then run `new_recovery_key`. That cuts off the owner's paper key too, and the takeover is permanent.
- **Gap with the docs.** The threat table says the paper thief "cannot do anything after the owner makes a new recovery key". But the thief can remove the owner's devices first, so the owner never gets to make one. Even when the owner does make a new key, a device the thief already added stays an owner. The stolen-phone row ("act as that device until it is removed") does not mention that the phone can make itself impossible to remove.
- **Fix:**
  - Require the platform's own unlock (biometric or PIN) before removing a device, making a new recovery key, or inviting someone.
  - Do not let Recovery remove existing owner devices. Let it only add a device, plus remove devices after a delay that the other devices can cancel.
  - Show a "household changed" alert on every device for every roster change.
  - Update the docs: after replacing the key, check the device list and remove anything unknown.

#### M4. Strangers can fill the server's disk and memory; the rate limiter can be exhausted. **Confirmed** (traced)

- Where:
  - `handlers.rs:147-229` (open registration, up to 20 households, no per-household quota)
  - `handlers.rs:587-620` (blobs up to 26 MB each, unlimited count)
  - `crates/server/src/limit.rs:108-113` (limiter map)
  - `docs/SELF-HOSTING.md:83`
- **What is wrong.**
  - With the default open registration, anyone who can reach the server can create a household (5 per hour per IP address, 20 in total). That locks out the real owner and gives unlimited storage through blobs and batches.
  - The limiter's map has one entry per address and only drops entries older than an hour. From an IPv6 /64 (2^64 addresses), the map grows without bound, and each request over 50k entries scans the whole map under a mutex. That costs CPU, and the per-address limits stop meaning anything.
- **Docs.** SELF-HOSTING.md says `--max-households` "stops strangers filling your disk". It does not.
- **Fix:**
  - Per-household byte quotas, plus a server-wide disk ceiling.
  - Default to requiring a registration token when listening on anything other than loopback (Q-08), or at least when the household count reaches 1.
  - Key the limiter on IPv6 /64 (or /56), cap the map with LRU eviction, and add a global bucket for unauthenticated endpoints.

### Low

#### L1. The "HTTPS only" address check can be bypassed with userinfo. Debug builds match hostnames by prefix. **Confirmed** (demonstrated)

- Where: `crates/sync/src/lib.rs:152-191`
- **What is wrong.**
  - `http://127.0.0.1:1@evil.example` passes the check, because `rsplit_once(':')` takes `127.0.0.1` as the host. The real host is `evil.example` (`scratchpad/urlcheck`, using `http::Uri`).
  - XHR in the web build follows URLs with credentials. ureq probably does too (not tested).
  - In debug builds, `host.starts_with("10.")`, `"192.168."` and `.ends_with(".local")` also accept names like `10.evil.com`.
- **Impact.** Plain HTTP to an arbitrary host, which exposes metadata. Content stays end-to-end encrypted.
- **Fix.** Parse with a real URL parser. Refuse userinfo. For the debug allowance, compare a parsed IP address against the private ranges.

#### L2. One bad or squatted file stops all file downloads. First upload wins on the server. **Confirmed** (traced)

- Where:
  - `client.rs:615-622` (`self.download(...)?` aborts the loop)
  - `client.rs:642` (`open_blob` error propagates)
  - `handlers.rs:610-614` (written only if absent; concurrent uploads share one `.tmp` path)
- **Attack.** A member (or the server) uploads garbage under a new file's name before the honest uploader does. The name is computable as soon as the attachment operation syncs.
  - The honest device sees "not missing" and never uploads the real file.
  - Every receiver fails the hash check, and every sync ends with a "security check failed" error.
  - Files queued after that one are never fetched.
  - Two concurrent uploads of the same name can also interleave into one corrupt file, with the same result.
- **Fix:**
  - Per file: skip it, record the failure and move on.
  - Make a hash mismatch re-upload-able: let an uploader that holds the file overwrite a blob that a receiver reported bad, or keep several candidates per name.
  - On the server, use a unique temporary file name (`.{name}.{random}.tmp`).

#### L3. Devices can permanently disagree in ways SECURITY.md says cannot happen. **Confirmed** (traced) for (a) and (b); **Plausible** for (c)

- (a) **Race at removal.** A removed device's batch with `seq > last_seq` is accepted by devices that pulled it before seeing the removal, and refused by devices that pulled it after (`client.rs:518-526`, `967-975`). The cursor moves past it either way.
  - The window between the remover's sync and its publish is small, but a malicious server can widen it at will.
  - An honest server could close it: refuse a removal whose `last_seq` is below the highest `device_seq` it holds for that device.
- (b) **Version skew.** A batch that fails `serde_json::from_slice::<Vec<Op>>` (for example, a new `EntityKind` from a newer app) is refused (`client.rs:538-540`). The cursor moves on, so even after upgrading, that device never gets those operations.
  - Fix: store batches that could not be parsed, and retry them after an upgrade.
- (c) **Duplicate op ids.** `apply_op` drops duplicates by `op_id` alone and ignores content (`store.rs:847-855`). Two different operations with one id resolve by arrival order. A server that reorders batches, together with a member who reuses an op id, therefore causes divergence.
  - Fix: store a hash of each op's content. If a second op arrives with the same id but different content, pick the winner deterministically (for example, the smaller content hash) and report it as tampering.
- SECURITY.md lines 189-190 ("skipped by every device the same way") should be qualified.

#### L4. A batch sealed before a rotation is still sent after the device learns of it. **Confirmed** (traced)

- Where: `client.rs:440-447`
- **What is wrong.** `pending` was sealed under the epoch current at the time. After `refresh` brings in a removal, the pending batch still goes out under the old epoch, so the removed member (with the server's help) can read those operations.
- **Impact.** The content was written before this device knew of the removal, so the leak is small, but easy to avoid.
- **Fix.** If `pending.header.epoch < chain.current_epoch()`, re-seal under the new epoch. Keep the same `device_seq`, use a new batch id, and on the server's "seq reused" answer treat the old batch as already sent.

#### L5. Server robustness. **Confirmed** (traced)

- Nonce cleanup runs only when `now % 64 == 0` (`lib.rs:339-341`). That depends on the clock and may rarely fire on coarse clocks. Use a counter or a timer.
- Every handler takes one global SQLite `Mutex`, and blob I/O and SQLite run blocking on tokio worker threads (`handlers.rs:609-613`, `635`). A few slow or large requests stall everyone. Use `spawn_blocking` and a connection pool, or a dedicated writer thread.
- `get_blob` reads up to 26 MB into memory per request.

#### L6. Pairing: the owner's "yes" alone adds the device, and the owner's screen shows a number even when the real new device shows an error. **Plausible** (a human-factors issue)

- Where: `crates/sync/src/pairing.rs:216-252`, `262-328`
- **Scenario.** Someone with a leaked code joins first. The genuine new device shows "Another device has already answered".
  - The owner's screen still shows a six-digit number and a device name the attacker chose, such as "Sam's phone".
  - A hurried owner taps Yes. The joiner's own confirmation is not needed for the attacker's device to be added.
- **Fix.**
  - Owner's screen: "Only say yes if the new device shows this same number. If it shows an error, say no."
  - Consider requiring the joiner's confirmation (a sealed "confirmed" message) before the owner can publish.
  - The docs' "the person confirms on both" overstates what is enforced.

#### L7. "Stop syncing here" does not securely erase the device key. **Plausible**

- Where: `crates/core/src/store.rs:606-609`
- **What is wrong.** `DELETE FROM sync_state` leaves the base64 device secret and key chain in SQLite free pages and the WAL until they are overwritten.
- **Fix.** `PRAGMA secure_delete = ON` for that statement, then a WAL checkpoint, or a `VACUUM`. Keystore wrapping (D-95 TODO) would make this moot.

### Informational: checked and found fine (or near enough)

- **Primitives and composition.**
  - Ed25519 with `verify_strict`; HPKE base mode, X25519 / HKDF-SHA256 / ChaCha20-Poly1305; XChaCha20-Poly1305 with random 192-bit nonces (reuse is negligible); HKDF and HMAC-SHA256.
  - The transcript encoding is unambiguous and labelled per use.
  - Purpose keys are domain-separated by purpose label, epoch and space salt.
  - No home-made primitives. No `unsafe` in scope.
- **Associated data and signatures.**
  - Batch AAD binds version, space, epoch, batch id, device, seq, `max_wall_ms` and nonce. The signature covers the AAD plus the ciphertext. Moving a batch to another household, epoch, device or seq fails.
  - Blob AAD binds space, name and epoch, and the plaintext hash is checked.
  - Envelope HPKE info binds space, recipient id, recipient X25519 key and epoch. Its signature covers space, recipient, epoch, sender, enc and ct.
  - Labels are bound to the device id, with a space-salted key.
  - HPKE base mode has no sender authentication by itself. It is correctly supplemented by the Ed25519 signature and a roster role check (`open_envelope`, `client.rs:979-1007`).
  - The anchor is AEAD under a key derived from the recovery key, bound to the space.
- **Roster.** Who may append is enforced on client and server: an owner or the recovery identity in the previous version. Entries are immutable, removals permanent and never re-added, and a removal forces epoch + 1.
- **Pairing.** 80-bit secret; the server sees only the 40-bit id. The commitment order is correct: the inviter commits `r` before the joiner's keys exist, and a middleman's offer must fix its key and `r'` before the real joiner's keys exist. A six-digit number gives 1 in 10^6 per invitation, and invitations need an owner action. Recording the offer and brute-forcing 2^80 offline would reveal only the household name and labels.
- **Recovery key.** 160-bit entropy, Base32 with a 20-bit checksum, HKDF derivation separated from the anchor key. Needs no stretching.
- **Requests.**
  - Signed over method, exact path-and-query, time, nonce and body hash.
  - ±5-minute window; nonces kept for 10 minutes in SQLite, inserted only after the signature verifies, deleted when the device is revoked.
  - Nested base paths are handled. The signature does not bind the host, which is fine.
- **Server input handling.** All SQL is parameterised. Entity table names come from an enum. Blob names, invitation ids and space ids are strict hex, so there is no path traversal. I found no panic reachable from attacker input in the server.
- **Clock guard.** Server and receivers check against `received_ms`, which the server chooses and does not sign. A malicious server can make chosen receivers refuse a batch (equivalent to withholding), or admit a future-dated batch a member produced. That is acceptable under "the server can deny service", but the docs could say so.
- **Debug-only overflow panics** on malicious server or member input:
  - `client.rs:117,134`: clock offset arithmetic when `hw-server-time` is near `i64::MAX`.
  - `primitives::unpad` (`4 + len`) and `KeyChain::from_bytes` (`4 + 32*n`) on 32-bit wasm.
  - Release builds wrap harmlessly. Use `checked_*` anyway.
- **Blob names never rotate** (epoch-0 key, by design, for dedup). A removed member working with the server can test whether a specific file it already knows about is stored.
- **Registration token** is compared with `!=`, not in constant time. A timing attack over the network is impractical; `subtle` is already a dependency.
- **Logging.** No secrets or plaintext are logged. Serde error messages echoed back to clients may quote the client's own input, which is harmless.
- **Zeroization.** Device secret, household keys, pairing secret and recovery key are wiped when dropped. JSON, HTTP and SQLite copies are not (SECURITY.md says so).
- **Dependencies** (Cargo.lock):
  - Versions: ed25519-dalek 3.0.0, curve25519-dalek 5.0.0, x25519-dalek 3.0.0, hpke 0.15.2, chacha20poly1305 0.11.0, hkdf/hmac 0.13.0, sha2 0.11.2, getrandom 0.4.3 (0.2.17 also present, native only), rustls 0.23.46, rustls-webpki 0.103.17, ring 0.17.14, ureq 3.5.2, axum 0.8.9, hyper 1.13.0, h2 0.4.21, tokio 1.54.1, idna 1.1.0, rusqlite 0.43.0.
  - `cargo audit` is not installed; I did not install it. Advisories I know of do not apply to these versions: curve25519-dalek < 4.1.3 timing (RUSTSEC-2024-0344), ring < 0.17.12 AES panic (RUSTSEC-2025-0009), idna < 1.0 (RUSTSEC-2024-0421).
  - Several of these crates (dalek 3/5, hpke 0.15, RustCrypto 0.11/0.13) are newer than my knowledge, so I cannot vouch for them, and SECURITY.md is right that they are unaudited at these versions.
  - Add `cargo audit` / `cargo deny` to CI.

## Design limitations acceptable if documented (and whether SECURITY.md does)

| Limitation | Documented honestly? |
|---|---|
| Server sees metadata (device count, roles, member grouping, timing, sizes rounded to 256 B / 4 KB, `max_wall_ms`) | Yes |
| Server can withhold, reorder or delete data (denial of service) | Yes |
| Fork or equivocation hides removals or new data (M1) | Yes in "Known weaknesses"; contradicted by the plain-language removal paragraph and the threat table |
| Removed members keep everything they already had, and with the server's help can read anything stored in old epochs (including old blobs) | Mostly. The "old blobs stay readable" part is implicit |
| No forward secrecy within an epoch: one current device compromise exposes the whole history (the chain is wrapped whole) | Implicit only; state it |
| Limited members are limited only in the interface (D-98) | Yes, honestly |
| Members can edit or delete anything; tombstones always win, so a member's delete is irreversible on every device (no undo except backups) | Partly ("members can rewrite shared data"); the irreversibility should be explicit |
| Roles inside the household data are not enforced | Yes |
| Recovery key = full owner power, including removing every other device (M3) | No: understated |
| Stolen unlocked owner phone = full takeover, including replacing the recovery key (M3) | No: understated |
| Keys at rest in plain app storage (Keystore and WebCrypto wrapping TODO) | Yes (D-95) |
| Safety number depends on the person's comparison | Yes; see L6 for what is actually enforced |
| Server restored from an old backup loses acknowledged batches | Yes |
| No per-household quota (M4) | Partly; SELF-HOSTING.md overclaims |

## Recommended order of work

1. H1 (per-device hash chain, plus `last_hash` in `RemovedEntry`; refuse a second batch at the same seq on receivers).
2. H2 (per-batch size cap, byte-budgeted pages, client limit fallback).
3. M2 (check `by` against the author; record-conflict rule).
4. M3 (re-authentication before admin actions; limit what the recovery identity can remove; change alerts).
5. Doc corrections: SECURITY.md lines 47-51, 79-85, 189-190, 205-207; SELF-HOSTING.md line 83.
6. M4, then the Low items.
