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
| `url` 2 (+ `idna`, `form_urlencoded`, `percent-encoding`) | parsing server addresses (after the security review, L1) | MIT OR Apache-2.0 |
| `tokio-util` 0.7 | streaming files from the server | MIT |

All are compatible with AGPL-3.0 (CDLA-Permissive-2.0 is a permissive data
licence for the root certificate list). No word list is bundled: the
recovery key is Base32 (D-91). Checked against `Cargo.lock` on 2026-11-28.
`deny.toml` lists the allowed licences; `cargo deny check` (in CI) checks
every dependency against it and against the RustSec advisory database.

## Phase 7: Home Assistant

The bridge (`crates/bridge`) adds no new Rust crates: it uses `axum`,
`tokio`, `tower-http`, `subtle`, `sha2`, `clap` and `ureq`, all listed
above.

The Home Assistant integration has **no runtime dependencies** of its own
(`manifest.json` has `"requirements": []`); it uses only what Home
Assistant ships (`aiohttp`, its helpers). Development only, never
shipped, installed in `integrations/home-assistant/.venv`:

| Package | Used for | Licence |
|---|---|---|
| `pytest-homeassistant-custom-component` 0.13.375 (brings `homeassistant` 2026.10.4, `pytest` and plugins) | the integration's tests | MIT (harness); Apache-2.0 (Home Assistant) |
| `ruff` | formatting and lint | MIT |
| `mypy` | type checks | MIT |
| `uv` (in `.bootstrap`, to fetch Python 3.14 when the system's is older) | setting up the venv | MIT OR Apache-2.0 |

The add-on image (`integrations/home-assistant/addon/housewright/Dockerfile`)
starts from Home Assistant's Debian base image (Apache-2.0) and installs
`jq` (MIT) to read its options.

## Phase 9: US product recall data

- **CPSC Recalls API** (`https://www.saferproducts.gov/RestWebServices/Recall`),
  US Consumer Product Safety Commission. A work of the US federal
  government; data.gov lists the Recalls API under the US public domain
  label (`http://www.usa.gov/publicdomain/label/1.0/`), checked 2026-11-28.
  No key or registration. The app downloads it on the person's device
  only when they switch recall checks on (D-141); nothing of it ships with
  the app. A recorded response of seven recalls is kept as a test fixture
  (`crates/core/tests/fixtures/cpsc/recalls-2026-10.json`), unchanged.
  The interface credits "US Consumer Product Safety Commission (public
  domain)" on the Recalls screen. CPSC's name and the recall notices are
  linked, never presented as Housewright's own.

No new Rust or JavaScript libraries were added in Phase 9.

## Library hookup: the Housewright appliance library

- **Housewright appliance library**, by Housewright appliance library
  contributors, <https://github.com/holdTheDoorHoid/housewright-library>,
  licensed under [Creative Commons Attribution 4.0 International
  (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/). The app
  ships version 2026.11.28, commit
  `d623c10820f220f6c3de8d415394bb271a18b416`, unmodified, in `library/`
  (with its licence in `library/LICENSE` and the version in
  `library/SNAPSHOT.toml`); `tools/library-sync` records the version and
  commit whenever it is refreshed. The app packs those files into one
  compressed file inside the program (D-147) and may download a newer
  published version (D-148). Credit appears in Settings, About (title,
  repository, licence, version and commit), at the foot of every
  "From <Brand>" section on a thing's page, and per fact as its source
  link and checked date. The library holds links to manufacturers' pages
  and facts in its contributors' own words, never manuals (D-127); the
  manuals and pages it links to remain their manufacturers' property, and
  brand names are their owners' trademarks, used only to identify
  products. CC BY 4.0 data shipped beside AGPL code is compatible: the
  data keeps its own licence.

No new Rust crates were added: `flate2` (MIT OR Apache-2.0, with
`zlib-rs`, Zlib) was already in the build through `zip` and is now also a
direct dependency of the core and of the API's build script; `ureq`
(MIT OR Apache-2.0) was already in the Android app through sync and is
now a direct dependency there for downloading manuals, and of
`tools/library-sync`.
