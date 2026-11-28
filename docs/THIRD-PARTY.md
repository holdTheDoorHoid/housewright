# Third-party code and data

Housewright is AGPL-3.0-only. Everything it bundles must be compatible with
that and redistributable. Data sets are in [DATA-SOURCES.md](DATA-SOURCES.md);
this file lists the Phase 4 additions (code libraries and model weights).
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

Checked with `cargo metadata` on 2026-11-27.
