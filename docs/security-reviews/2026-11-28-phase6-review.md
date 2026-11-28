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

---

## Verification pass (commits 5a2be8b, 4331a24, 94fb0dd; HEAD 94fb0dd)

**Verdict: ship after fixes.** Most of the original findings are now fixed correctly. The fixes introduced one new High: any member can permanently stop every other device from sending changes. They also introduced two new Mediums, and both lose data.

**How I checked.**
- I read the full diff against `b0b650f`.
- I ran the sync-related crates' tests: crypto 23, core 142, server 2, sync 27 including `review.rs` 12/12. All pass. I did not run `housewright-api` or the app tests, so I did not reproduce the builder's figure of 256 tests.
- I wrote four new attack tests in a scratch crate (`scratchpad/verify/tests/v.rs`). It reuses the project's own test harness by `#[path]` and changes nothing in the worktree.
- My `forge/` scenario is now covered end to end by `review.rs::h1_a_removed_device_and_the_server_cannot_forge_batches`. That test uses the new batch format, includes a device that joins later and is shown the forged batches first, and passes. I re-ran it and checked its logic, rather than keeping a second copy.

### Status of each original finding

| Finding | Status | Notes |
|---|---|---|
| **H1** forged batches from a removed device | **Fixed** | See the bypass checks below. |
| **H2** page too large wedges sync | **Fixed** | See H2 notes below. One leftover belongs under N2. |
| **M1** fork / withholding | **Mitigated, documented** | Batches now carry the author's roster version and hash. A restoring device still trusts the newest roster it is shown; SECURITY.md now says so. The new gossip itself creates **N1**. |
| **M2** record overwrite and forged `by` | **Fixed** | See M2 notes below. |
| **M3** takeover | **Fixed for a stolen phone; documented for the paper** | See M3 notes below. |
| **M4** disk filling and rate limiter | **Fixed** | See M4 notes below. One new problem is **N4**. |
| **L1** server address check | **Fixed** | Uses the `url` crate. User names and passwords are refused. The debug allowance now checks parsed private IP addresses. |
| **L2** squatted file | **Mitigated** | One file failing no longer stops the others, and temporary file names are unique. The heal-by-delete design introduces **N3**. |
| **L3** divergence | (a) **Fixed** with an honest server (`removal_stale`); (b) **Fixed**: unreadable batches kept and retried with full checks; (c) **Mitigated**: same op id with different content is now a counted conflict, first-applied wins; order dependence under reordering is documented | |
| **L4** pending batch resealed | **Fixed** | Same `device_seq` and same `prev`, new batch id. The server's `seq_reused` answer correctly makes the original the device's head. Receivers can't be given both, because the server enforces one batch per number, and if a hostile server did serve both, the second is refused as tampering. |
| **L5** server robustness | **Fixed** | `spawn_blocking`, streamed file downloads, timed nonce cleanup. |
| **L6** pairing confirmation | **Partly** | See L6 notes below. |
| **L7** secure erase | **Fixed** (plausible; the builder's `l7` test passes) | `secure_delete` around sync-state writes, plus a WAL checkpoint on "stop syncing". |

**H1: the chain rules hold.**
- The `prev` hash is part of the AAD and the signature.
- A current device's batch is accepted only at `head.seq + 1` with `prev == head.hash`. Any other batch at a number already used is counted as tampering.
- A removed device's batches are held and applied only once the chain from the receiver's head back to `last_hash` is complete. A forged batch cannot be on that chain without a SHA-256 second preimage.
- Edge cases I checked:
  - `last_seq = 0`: nothing is accepted.
  - A first batch at seq 1 with a zero `prev` from a removed device: held, never applied.
  - A fork the removed device made of its own chain: never reaches `last_hash`.
  - A gap in the chain: refused.
- The server also enforces the chain on push.
- The founding-device exception only relaxes authorship, not the chain rules (but see N6).

**H2: paging.**
- The server refuses any batch over 1 MB, pages stop at about 8 MB, and the client halves the page size and retries.
- The leftover: a server can mark any real batch `oversize`. The client then skips it, which breaks that device's chain (N2). So "skip" can be used to cut a device off permanently, though it does show in the skipped and tampered counts.

**M2: record binding.**
- `by` is checked against the device's member, and records are now first-applied-wins. The backdating overwrite is gone.
- Convergence:
  - Honest devices apply batches in server order (the author applies its own copy first, and that also comes first in server order), and record ids carry 74+ random bits. So no honest conflicts.
  - A member cannot plant an id first, because it only learns the id after the batch is sent.
  - The order dependence is only adversarial: it needs a reordering server, or batches applied out of order (held or unreadable ones) colliding with a planted id. SECURITY.md documents this honestly.
- What remains is N6.

**M3: takeover.**
- `verify_next` (run on clients and on the server) requires the previous recovery identity's co-signature to remove an owner device or the recovery identity.
- A roster signed by the recovery identity may only add devices.
- The server cannot forge the co-signature, and the recovery identity cannot use it to remove anyone.
- What remains, and is documented:
  - The paper is a master key: its holder adds an owner device, then co-signs.
  - If the paper is lost, a stolen owner phone cannot be removed at all. The documented way out is setting up a new household.
- Notices of roster changes are shown on every device.

**M4: registration and quota.**
- A registration token is now made by default and compared in constant time.
- There is a 2 GB quota per household, the limiter counts IPv6 by /64, and memory is bounded.
- Two small costs:
  - The quota is a full `SUM()` scan on every batch push (cost grows with history).
  - The token is printed to the server log at every start (documented).

**L6: pairing confirmation.**
- The owner can no longer finish before the new device confirms.
- But the confirmation is a constant (`"numbers match"`) sealed with the pairing code. It is not bound to the joiner's keys or the safety number.
- So in the scenario I described (someone with a leaked code joins first), that attacker sends the confirmation itself. The owner's screen then shows `other_confirmed: true`, which may reassure a hurried owner.
- Protection still rests entirely on the person comparing the two screens.

### New findings

#### N1 (High). Any member can permanently freeze every other device's outgoing sync. **Confirmed** (demonstrated)

- Where: `crates/sync/src/client.rs:851-865` (gossip handling); `client.rs:473-476` (push refused while `fork` or `newest_roster_seen > latest`); `state.rs:85-88` (both fields are persisted, and nothing ever clears them).
- **What is wrong.** The `roster_version` and `roster_hash` inside a batch body are whatever the author wrote. Nothing checks them.
- **Attack.** A Member or Limited device (a modified app) pushes one validly signed and chained batch containing either:
  - `roster_version = u64::MAX`; or
  - `roster_version = 1` with a wrong hash.

  Every other device then refuses to send anything, permanently (`RosterHidden`). Its edits pile up locally, and pulling still works.
  - `scratchpad/verify/tests/v.rs::n1_*`: both variants make two syncs in a row on Ana's phone return `Err(RosterHidden)`.
  - There is no way back without editing the device's database.
- **Fix:**
  - Do not trust a version number on its own. Put the claimed `SignedRoster` (or its chain from the receiver's newest version) inside the batch, and verify it.
  - Treat a claim that can't be verified, or a hash mismatch, as tampering by *that author*: say so, don't freeze the device.
  - Clear `newest_roster_seen` once that version arrives, and let the person acknowledge or clear a fork warning.

#### N2 (Medium). One missing batch cuts a device off from everyone else, permanently. A server restored from backup wedges every device. **Confirmed** (demonstrated)

- Where: `client.rs` `receive()`, the gap check at `seq != head.seq + 1 || prev != head.hash` (rejected, head not advanced, cursor moves on); `handlers.rs` `push_ops` chain check (refuses if the server lacks `device_seq - 1`).
- **Scenario 1: one lost batch.** The server loses, withholds once, or marks oversize a single batch from device E. Every device that had not yet pulled it then refuses *every later batch from E*, forever, counting each as tampering.
  - `v.rs::n2_*`: after one of E's batches was deleted from the server database, neither Ana nor Ben ever received E's later changes (rooms 13 and 14).
- **Scenario 2: old server backup.** The server is restored from an old backup (an honest operator, covered in SELF-HOSTING.md).
  - Every device whose recent batches were lost now has its next push refused with `chain`. Its pending batch never goes, so its sync stays wedged.
  - Receivers also see gaps, as in scenario 1.
  - SECURITY.md's "not handled" understates this: it is now total and permanent, not "a few batches lost".
- **Fix:**
  - On a gap, hold the batch (as is done for removed devices), fetch the missing numbers by device and number (new endpoint), and apply once the chain is complete. Refuse only a *different* batch at a number already seen.
  - When the server's `chain` error means it is missing earlier batches, the device should re-push its own chain from what it holds locally. Keep sent batches locally for this.
  - Show the person a clear "missing changes from X" message.

#### N3 (Medium). A device with stale keys deletes good files from the server; any member can delete any file. **Confirmed** (demonstrated)

- Where: `client.rs` `download()` (any `open_blob` error leads to `DELETE`); `handlers.rs` `delete_blob` (any member, any blob, no condition).
- **Scenario 1: honest data loss.**
  1. Ben's phone has not synced since Ana removed a device (a new epoch).
  2. Ana attaches a photo, which is sealed under the new epoch.
  3. Ben opens it (`fetch_file` does not refresh keys).
  4. Decryption fails with "no key for epoch 1", and Ben's phone deletes the good file from the server.
  - `v.rs::n3_*`: server blob count went 1 → 0.
- The same happens in `sync_files` if a rotation lands between refresh and download.
- If no device still holds the file (files over 4 MB are not prefetched; the device that uploaded it was lost), it is gone, including from any later recovery.
- **Scenario 2: malicious member.** A member can delete every file on the server deliberately.
- **Fix:**
  - Refresh keys and retry before deciding a file is bad.
  - Ask for a delete only when decryption *authenticated* and the hash then failed, or the AEAD tag failed. Never on "no key for this epoch".
  - Better: never delete. Let a device that holds the real file replace a blob that was reported bad (keep the old copy until a valid replacement arrives), and restrict DELETE to owners, or remove it.

#### N4 (Low). One address can drain the global rate limit and lock everyone out of pairing, recovery and registration. **Confirmed** (traced)

- Where: `crates/server/src/limit.rs` `allow()`. The global bucket's token is taken *before* the per-address check, so refused requests still spend global tokens.
- **Attack.** One client sending more than 300 requests a minute to the pairing endpoints (more than 100 to recovery, more than 20 an hour to create) locks out every other user.
- **Fix:**
  - Check the per-address bucket first, and take a global token only if it passes.
  - Consider a global bucket that only applies once many distinct addresses are active.

#### N5 (Low). A malicious server can fill a device's storage with "held" batches. **Confirmed** (traced)

- Where: `client.rs` `receive()`, the `Author::Removed` branch.
- **What is wrong.**
  - It stores `held:{dev}:{hash}` for any batch naming a removed device with `seq <= last_seq`, *before* checking the signature, and with no cap.
  - `resolve_held` then rescans every held key each time.
  - So the server can feed unlimited batches of about 1.4 MB each, with costs growing quadratically.
- **Fix.**
  - Run `verify_batch` against the removed entry's key before holding.
  - Cap the held count at `last_seq - head.seq`.
  - Keep at most one held batch per number. Since signatures are now checked first, a second, different one is a fork by the removed device itself.

#### N6 (Low). The founding device's exception never closes. **Confirmed** (traced)

- Where: `client.rs` `open_checked`: `pre_sync = founder && o.hlc.wall_ms < genesis.made_ms`.
- **What is wrong.** The founding device can still write operations, and records whose `by` is someone else, in any member's name, at any time, just by backdating them before setup. Nothing limits it to sending them once.
- This contradicts the claim in `review.rs` and SECURITY.md that nobody, an owner included, writes in another member's name. Impact is low, because the founder is an owner.
- **Fix.** Accept the exception only in batches up to a number the founder records in the genesis roster (for example `history_upto_seq`), or only for operations whose ids are listed in a hash committed at setup.

### Things checked and found sound

- The co-signature transcript is separated from other signatures, covers the exact body bytes (including version and `prev`, so it can't be replayed), and is included in the roster hash.
- `last_hash` is validated as hex.
- The server's `removal_stale` check uses the stored chain head.
- `unpad` and `KeyChain::from_bytes` now use checked arithmetic, and the clock-offset arithmetic saturates.
- Unreadable batches are retried with every check applied again.
- `secure_delete` is turned on only around sync-state writes.
- The quota is enforced for both batches and files.

### Recommended before shipping

1. N1: verify gossip claims, never freeze on an unverifiable one, and let the person clear the state.
2. N2: hold and refetch on chain gaps; re-push own history after a server restore.
3. N3: no delete on "no key", refresh first; replace instead of delete; owners only.
4. Then N4, N5, N6, and binding the L6 confirmation to the safety number (seal `H(safety number inputs)` rather than a constant, so it at least proves the joiner computed the same number).

---

## Second verification pass (commit 3d058c3)

**Verdict: ship after fixes.** Three new Medium problems, all demonstrated. None of them exposes content; H1 still holds.

**How I checked.** I read the diff from `94fb0dd` to `3d058c3`. I ran three new attack tests in `scratchpad/verify/tests/v.rs`, which reuses the project's test harness and changes nothing in the worktree. I did not re-run the builder's own suite (claimed 264 passing).

### Status of each item

| Item | Status | Notes |
|---|---|---|
| **N1** fake roster claims from members | **Fixed for members; still open for owner devices** (P1) | A member can no longer invent a version: a claim counts only if signed by a manager device. But a claim newer than what the device has is accepted on the signature alone, without checking it links to the chain. |
| **N2** one missing batch | **Fixed** | See N2 notes below. Leftover: P3. |
| **N3** stale keys deleting files | **Partly fixed** | DELETE is gone, and a device with stale keys now refreshes and retries. But "mark bad, then upload" lets a member replace the only copy of a file (P2). |
| **N4** global rate limit drained | **Fixed** | The per-address check now runs first. |
| **N5** held-batch flood | **Fixed** | The signature is checked before holding, with at most 4 per number and `(last_seq - head) × 4` in total. Leftover: L-a. |
| **N6** founder exception | **Fixed for new households** | `history_batches` and `history_head` are recorded in genesis and checked. Restore and pairing read the same genesis, so they behave consistently. No migration: see L-b. |
| **L6** pairing confirmation | **Improved, not closed** | It is now bound to the safety-number inputs. But an attacker who joined first with a leaked code can compute it for its own keys, so it is accepted. Protection still rests only on the person comparing the two screens; that is fine as long as the docs say so. |
| Unbounded storage | **Documented** (SECURITY.md:505, D-107) | Every sealed batch is kept (`sent:*`), plus one `seen:` key per batch received. No pruning. |

**N2: gap filling.**
- A gap is held, and missing batches are fetched with `/chain/{dev}/{seq}`.
- A fetched batch is checked for device, number, `prev` link and signature, so this new endpoint cannot inject or replace a batch, and H1 holds.
- Each device re-sends its own batches from its local copies.

### New findings

#### P1 (Medium). A stolen owner phone, or anyone with the recovery paper, can freeze every device's sending. **Confirmed** (demonstrated)

- Where: `crates/sync/src/client.rs`, the "newer than anything the server gives us" branch of `check_claim` (around line 1175), and `push()` at line 535.
- **What is wrong.** A claim with a higher version than the device has is believed on the owner's signature alone. It is never checked to link to the chain. `newest_roster_seen` is never cleared (`dismiss_notices` clears only `fork`).
- **Attack.** The owner tablet, with no paper, signs a copy of the roster body with `version = 999999` and puts it in one batch.
  - `p1_*`: Ana's phone returns `RosterHidden` on every sync, including after "dismiss".
  - This undoes M3's promise that a stolen owner phone can't do lasting harm without the paper.
- **Fix:**
  - Only believe a newer claim that can be verified: carry the versions between, check them with `verify_next` from the device's newest roster, and require each to be newer.
  - Otherwise count it as tampering by that device.
  - Also cap `newest_roster_seen` to versions that were verified, and let the person clear it.

#### P2 (Medium). A member can replace the only copy of a file on the server with garbage. **Confirmed** (demonstrated)

- Where: `crates/server/src/handlers.rs`, `report_bad_blob` and `put_blob_sync` (`if !path.exists() || bad`, then rename over the old copy).
- **What is wrong.**
  - Any member can mark any file bad, then upload garbage, which overwrites the server's copy (`p2_*`).
  - Honest devices that still hold the file will heal it. But a file that exists only on the server is lost: files over 4 MB aren't prefetched, and the uploading device may be gone. That includes recovery.
  - The doc comment "the old one is kept until then" is true only until the *attacker's* upload, which comes straight after the mark.
- **Fix:**
  - Keep several copies under each file name and never overwrite. Serve all of them, or the newest valid-looking one plus a list; the client tries each against the hash and reports which one was good.
  - Or accept a replacement only from the device that uploaded the original.

#### P3 (Medium). A server restored from a backup taken before a removal wedges every device, and the removed device is un-revoked. **Confirmed** (demonstrated)

- Where:
  - `client.rs` `restore_rosters` (re-posting the removal version fails with `removal_stale`, because the restored server lacks the removed device's later batches, which only that device holds);
  - `handlers.rs` `update_roster_sync`, the `removal_stale` check;
  - `push_ops`, which refuses member batches under the new epoch (`h.epoch > epoch`).
- **Observed** (`p3_*`):
  - The owner gets `removal_stale` on every sync, and the member gets "a batch uses a key epoch the household does not have" on every sync.
  - Ben's post-removal change never reached Ana.
  - The server's `devices.revoked` for the removed tablet is 0 again. The tablet can push, and pull ciphertext it cannot read. Clients still refuse its new batches (H1 holds).
- **Fix:**
  - When re-posting lost versions after a restore, skip the `removal_stale` check, or apply it only to new removals. The removal's signed `last_seq`/`last_hash` already bind what clients accept.
  - Re-apply revocations from the re-posted versions before accepting any other request.
  - Let members' batches wait (instead of failing) until an owner has repaired the roster.

### Low

- **L-a. A removed device working with the server can block its own real batches.** **Plausible.**
  - Where: `client.rs` `receive()`, the `Author::Removed` branch (the 4-candidates-per-number cap).
  - The removed device signs 4 forks per number, and the server delivers them before the real batch. The real one is then refused as "full", so its chain never completes on that receiver.
  - Its pre-removal changes are lost there: divergence. No content leaks.
  - Fix: when the slots for a number are full, fetch the batch whose hash is the `want` hash while walking back from `last_hash`, rather than refusing it.
- **L-b. Households created before 3d058c3 have no history commitment.** **Confirmed** (traced).
  - Their founding device's pre-sync batches, which carry other members' names, are now refused by newly joined devices, while older devices already applied them.
  - Phase 6 is unreleased, so this only matters for test households. Fix: a one-off migration, or document that pre-3d058c3 households must be set up again.
- **L-c. A server can hide a removal from one device if it also hides the remover's later batches from that device.** That device then only shows "missing changes from …" and keeps sealing under the old epoch. This residual is known; SECURITY.md should say "missing changes" is a warning to act on.

---

## Third verification pass (commit 0c7ccba)

**Verdict: ship after fixes.** Two Medium problems remain. Both are availability or storage abuse; neither exposes data. Each is a small, local fix.

**How I checked.** I read the diff and re-ran the sync, crypto and server tests: all pass (sync 37, including `review`, `verify` and `verify2`). I ran two new tests in `scratchpad/verify/tests/v.rs` (q1, q2).

### What holds

- **`restore: true` can't be abused.**
  - The flag only skips the server's `removal_stale` check.
  - `verify_next` still runs: next version only, linked to the previous one, signed by a manager, and co-signed whenever an owner device or the recovery identity is removed.
  - So nobody can use it to undo a removal, replay an old version, or bypass the co-signature, whether a member, an owner device or the server.
  - The worst case: an owner device that sets the flag on a *new* removal re-opens the L3a race. Owners are trusted; that is a Low.
- **"Pause sending" from claims of a newer version (P1)** is now bounded.
  - A newer version must link to the device's own chain through `verify_next`, and only the last 8 versions are carried.
  - A stolen owner phone can therefore cause at most about 8 pauses this way, one clearable notice each.
  - M3 still holds: removing an owner device still needs the co-signature.
- **Per-device file copies.**
  - No device can overwrite another device's copy.
  - The client tries every copy listed in `hw-copies` against the hash in the signed record, so a bad copy listed first doesn't win forever. Only the server can hide copies, which is denial of service it could do anyway.
- **`epoch_ahead`** waits until an owner's device repairs the server. That wait is permanent only if no owner device ever syncs again, or if a lost recovery-key change needs the paper. Both are documented.
- **L-a** walk-back fetching is correct: the fetched batch's number, device, hash and signature are all checked.

### Medium

**Q1. A stolen owner phone (no paper) can pause every device's sending again after every OK, indefinitely.** **Confirmed** (q1)

- Where: `crates/sync/src/client.rs`, the same-number branch of `check_claim` (around lines 1197-1214).
- **What is wrong.** A claimed version with the *same* number as the device's own, but a different hash, counts as a fork if a manager of the previous version signed it. It is never checked with `verify_next`, and every new one sets `fork = true` again.
- **Attack.** The phone ships a freshly signed copy of the current roster body (only `made_ms` changed) in each batch.
  - Observed: three rounds in a row of `RosterHidden` with `fork=true`, each straight after "OK".
  - Removing that phone needs the paper. Without the paper, the household's sending is paused until it is set up again.
- The same works for a removed owner device whose signed claim is carried by a current member.
- **Fix:**
  - Require the claimed same-number version to pass `verify_next` from version − 1.
  - Pause at most once per signing device, until the person acts. After an OK, show a notice and keep sending.
  - Do not count signers that are no longer in the newest roster.

**Q2. Re-uploading a device's own file copy skips the household quota.** **Confirmed** (q2)

- Where: `crates/server/src/handlers.rs`, `put_blob_sync`. The quota is checked only when `!path.exists()`; an existing copy is overwritten and `blob_copies.size` updated without a check.
- **Attack.**
  - At the quota limit, a new file name is refused (507), but overwriting an existing own copy with 20 MB is accepted (200).
  - A member uploads many 64-byte copies, then grows each to about 26 MB. Disk use is unbounded.
- **Fix.** Always check `usage − old_size + new_size <= quota`, inside the same lock or transaction as the insert.

### Low (one line each)

- `restore: true` on a fresh removal re-opens the L3a race (owner-only).
- With the paper lost and no owner device left, members stay in `epoch_ahead` waiting for good (documented).
- `put_blob` still reads the quota (`usage()` with `SUM`) and writes in separate steps, so two parallel uploads can both pass the check.

---

## Final check (ddfb9cd, 30b0bc7, e3bfc0e)

**Verdict: ship.** Nothing of Medium or higher remains.

- **Q1: fixed.** My q1 test now shows one pause, then sending continues after the OK (`fork=false` on later rounds).
  - A same-number claim must pass `verify_next` from version − 1.
  - Its signer must be a manager in the newest verified roster.
  - An OK'd signer never pauses sending again.
  - The attacker can't reset this. A new device id needs a published roster version that adds it: every device is told, and removing an owner device needs the paper anyway. This is documented as "set up again".
- **Q2: fixed.** My q2 test: overwriting your own copy at the quota now gets 507. The difference is charged, and the check and write happen under the database lock.
- **Restore race: fixed.** With `restore`, a removal is refused if the server holds *more* batches from that device than the removal names.
- **Pairing reorder (e3bfc0e).** The member record is pushed by the owner in the current epoch before the device is added. Nothing new leaks, and the H1 chain rules and M2 authorship rules are unchanged.
- **Tests.** All sync tests pass (including `review` 12, `verify` 8, `verify2` 8), plus my q1/q2.

### Low (one line each)

- A stolen owner phone can add new owner devices (visible notices), each of which can pause sending once.
- Once a signer is OK'd, a real roster the server hides is no longer flagged by pausing when that signer reports it, only by a notice. Worth one line in SECURITY.md.
- If the pre-pairing `sync()` fails, `invite_confirm` drops the open invitation, and the person has to start pairing again.
