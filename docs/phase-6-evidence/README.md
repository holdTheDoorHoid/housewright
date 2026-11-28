# Phase 6 evidence: sync and encryption

## Web (Playwright, production web build)

`app/tests/sync.spec.ts`, 2026-11-28: a real `housewright-server` on
127.0.0.1 and two browser profiles (separate storage), in Chrome. Passes
(about 12 s).

| File | What it shows |
|---|---|
| `web-sync-recovery-key.png` | The recovery key, shown once, with the plain warning; "Done" stays disabled until "I've written it down or printed it" is ticked. |
| `web-sync-invite.png` | "Add a device or person": the code, the QR code, the server, "works once, for ten minutes". |
| `web-sync-pair-owner.png`, `web-sync-pair-new.png` | Both profiles show the same six-digit safety number. |
| `web-sync-a.png` | The owner's sync screen at the end. |
| `web-sync-b-removed.png` | The removed profile: "This device was removed from the household". |

The test also: refuses `http://example.org` with a plain message; edits on
both profiles while the server is stopped ("Can't reach the sync server",
"changes waiting to send"), restarts it and converges; carries a photo
across (thumbnail decodes on the other profile); greps every file in the
server's data folder for the household's strings (none found); removes the
second profile, which then stops syncing and does not get the owner's
later change.

## Android emulator (`android/`)

Android 15 emulator (`hw_api35`, x86_64) through `~/Android/emu-adb` only,
2026-11-28. The x86_64 debug APK was installed with `install -r` over the
Phase 5 build, so the existing household (schema 4, with photos) migrated
to schema 5 in place. The sync server ran on the host at 127.0.0.1:8787;
the emulator reached it as `http://10.0.2.2:8787` (allowed in debug builds
only, D-96; the app's own HTTP client is used, so no Android cleartext
configuration was needed or added). A browser profile (Chrome, driven by
`app/tests/emu-browser.mjs`) served from `vite preview` was the second device.

| Check | Result | Evidence |
|---|---|---|
| Upgrade over existing data | **Pass.** Today intact after install (the "Pixel Launcher isn't responding" dialog is the headless emulator's own). | `01-launch.png` |
| Sync off by default; set up against the host's server | **Pass.** | `02-sync-off.png`, `03-recovery-key.png`, `06-sync-on.png` |
| Recovery key printed on the sealed volume | **Pass.** The recovery page shows the server and the key in a box, with the restore steps and the warning. The key wraps across two lines at a dash on a phone-width page; readable, but could be kept on one line. | `05-sealed-recovery-page.png` |
| Pairing a browser profile with the emulator's code | **Pass.** Same safety number on both (`063 603`), both confirmed, the browser got the household (Phase 3 to 5 data, the emulator's new "Garage door opener"). | `07-invite-code.png`, `08-compare-number.png`, `browser-b1-compare.png`, `browser-b2-things-after-join.png` |
| Edit on one, see it on the other | **Pass** both ways: the emulator's new thing on the browser; the browser's new "Sump pump" on the emulator (3 became 4 on both). | `10-things-with-browser-edit.png`, `browser-b4-things-final.png` |
| Devices list, removal | **Pass.** Names, people, roles, last seen; removing asks first; the removed browser profile then shows "removed from the household". | `11-devices.png`, `12-remove-confirm.png`, `13-after-removal.png`, `browser-b5-sync-screen.png`, `14-only-this-device.png` |

Honest notes: it took four pairings to get through the script. The first
browser session died on a bug in the driving script (it went to the sync
screen while the app was still navigating to the new thing); the second
failed because the emulator's "They match" button sat under the tab bar
and the tap hit "Today", which left the screen and cancelled the
invitation as designed; the third died on the same script bug. The
devices those sessions left in the roster were removed from the emulator
at the end, which exercised removal three more times. No app bug was found
on Android in this run.

Size: the x86_64 debug native library is 120.4 MB stripped (114.6 MB in
Phase 5): about +5.8 MB for rustls, ring, ureq and the cryptography.

Not checked on the emulator: background sync (not built), restore with the
recovery key on Android (tested in Rust and through the API), a real phone,
and an HTTPS server with a real certificate from the phone.
