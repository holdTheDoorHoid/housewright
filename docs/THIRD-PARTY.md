# Third-party code and data

Housewright is AGPL-3.0-only. Everything it bundles must be compatible with
that and redistributable. Data sets are in [DATA-SOURCES.md](DATA-SOURCES.md);
this file lists the Phase 4 and Phase 5 additions (code libraries, model weights and fonts).
Earlier dependencies (SQLite, rusqlite, jiff, serde, Tauri, Svelte and so on)
are permissively licensed (MIT, Apache-2.0, or public domain for SQLite).

## Model weights

| What | Where | Licence | Notes |
|---|---|---|---|
| ocrs text detection and recognition models | `models/ocr/` | CC BY-SA 4.0 (inherited from Google's HierText dataset) | Attribution in `models/ocr/README.md` and in the app's About text. Unmodified. CC BY-SA 4.0 is one-way compatible with GPLv3 (Creative Commons, 2015), and AGPLv3 allows combination with GPLv3 works; in any case the models are data shipped beside the program, not compiled from it. Anyone who modifies the models must share them under CC BY-SA 4.0. |

## Rust libraries added in Phase 4

All are permissive and compatible with AGPL-3.0.

| Crate | Used for | Licence |
|---|---|---|
| `image` 0.25 (+ `zune-jpeg`, `png`, `image-webp`, `fdeflate`, `moxcms`, `pxfm`, `bytemuck`) | decoding photos, downscaling, JPEG encoding | MIT OR Apache-2.0 (zune: also Zlib; moxcms, pxfm: BSD-3-Clause OR Apache-2.0) |
| `qrcode` 0.15 | drawing QR codes | MIT OR Apache-2.0 |
| `rqrr` 0.11 (+ `g2p`, `lru`) | reading QR codes from photos (a Rust port of quirc) | (MIT OR Apache-2.0) AND ISC |
| `pdf-writer` 0.15 | the sticker sheet PDF | MIT OR Apache-2.0 |
| `ocrs` 0.14 | on-device text recognition | MIT OR Apache-2.0 |
| `rten` 0.28 and its `rten-*` crates (+ `rayon`, `num_cpus`, `smallvec`, `typeid`) | running the OCR models | MIT OR Apache-2.0 |
| `zip` 9 (+ `flate2`, `zlib-rs`, `crc32fast`) | the full backup file | MIT (zip), MIT OR Apache-2.0 (flate2), Zlib (zlib-rs) |
| `sha2` 0.11 | content hashes of stored files | MIT OR Apache-2.0 |
| `base64` 0.22 (Tauri app only) | file bytes across the Android WebView bridge | MIT OR Apache-2.0 |

Checked with `cargo metadata` on 2026-11-27.

## Phase 5: the house binder

### Fonts

| What | Where | Licence | Notes |
|---|---|---|---|
| Lato Regular and Bold 2.015, by Łukasz Dziedzic (tyPoland) | `crates/binder/fonts/` | SIL Open Font License 1.1 | Unmodified (copied from Debian's `fonts-lato`). Licence and copyright in `crates/binder/fonts/OFL.txt`. Compiled into the app and embedded (subset) in every binder PDF, which the OFL allows; the OFL does not apply to the documents made with them. Reserved Font Name "Lato": we do not modify or rename the fonts. Never fetched from the network. |

### Rust libraries

| Crate | Used for | Licence |
|---|---|---|
| `typst`, `typst-library`, `typst-layout`, `typst-eval`, `typst-realize`, `typst-syntax`, `typst-utils`, `typst-macros`, `typst-timing`, `typst-html`, `typst-assets` 0.15 | typesetting the binder | Apache-2.0 |
| `typst-pdf` 0.15 and `krilla` 0.8 | writing the PDF | Apache-2.0 (typst-pdf); MIT OR Apache-2.0 (krilla) |
| `comemo`, `ecow`, `rustybuzz`, `ttf-parser`, `hypher`, `unicode-*`, `hayro-*`, `resvg`/`usvg`, `tiny-skia`, `syntect`, `two-face`, `hayagriva`, `citationberg`, `biblatex`, `wasmi` and their dependencies (about 225 crates in all) | Typst's own dependencies: memoisation, text shaping, hyphenation, images, and features the binder does not use (code highlighting, bibliographies, plugins) but Typst always compiles | MIT, Apache-2.0 or both for most; also Unicode-3.0 (the `icu_*`, `zerovec`, `yoke`, `tinystr`, `writeable`, `litemap` crates: ICU4X data and code), BSD-2-Clause (`arrayref`, `kamadak-exif`, `mutate_once`), BSD-3-Clause (`tiny-skia`), Zlib (`slotmap`), Unlicense OR MIT (`csv`, `walkdir`, `same-file`, `aho-corasick`), 0BSD OR CC0-1.0 (`roman-numerals-rs`) |

All are permissive and compatible with AGPL-3.0. Checked with `cargo metadata`
on 2026-11-28 (every crate added to `Cargo.lock` by `housewright-binder`).

## Phase 6: sync and encryption

| Crate | Used for | Licence |
|---|---|---|
| `ed25519-dalek` 3, `curve25519-dalek` 5, `x25519-dalek` 3, `ed25519` 3, `signature` | signatures, key agreement | BSD-3-Clause (dalek crates); Apache-2.0 OR MIT (`ed25519`, `signature`) |
| `hpke` 0.15 | RFC 9180 key wrapping | MIT OR Apache-2.0 |
| `chacha20poly1305` 0.11, `chacha20`, `poly1305`, `aead`, `cipher`, `universal-hash`, `inout`, `hybrid-array`, `crypto-common` | encryption | Apache-2.0 OR MIT |
| `hkdf` 0.13, `hmac` 0.13 | key derivation | MIT OR Apache-2.0 |
| `zeroize` 1.9, `subtle` 2.6, `rand_core` 0.10 | wiping secrets, constant-time helpers | Apache-2.0 OR MIT; BSD-3-Clause (`subtle`) |
| `base64` 0.22 | wire encoding | MIT OR Apache-2.0 |
| `axum` 0.8, `axum-server` 0.9, `tokio` 1, `hyper` 1, `tower` 0.5, `tower-http` 0.6 and their dependencies | the server | MIT |
| `rustls` 0.23, `ring` 0.17, `webpki-roots` 1, `rustls-webpki`, `tokio-rustls` | TLS in the server and the native client | Apache-2.0 OR ISC OR MIT (rustls); Apache-2.0 AND ISC (ring); CDLA-Permissive-2.0 (webpki-roots: Mozilla's root list); ISC (rustls-webpki) |
| `ureq` 3 | the native HTTP client (Android, desktop) | MIT OR Apache-2.0 |
| `clap` 4 | server flags | MIT OR Apache-2.0 |

All are compatible with AGPL-3.0 (CDLA-Permissive-2.0 is a permissive data
licence for the root certificate list). No word list is bundled: the
recovery key is Base32 (D-91). Checked against `Cargo.lock` on 2026-11-28.
