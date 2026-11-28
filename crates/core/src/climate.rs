//! Climate from a US ZIP code, offline (D-64).
//!
//! The table in `crates/core/data/us-zip-climate.bin` maps each Census ZIP
//! Code Tabulation Area (ZCTA) to the 1991-2020 NOAA climate normals of its
//! nearest temperature station: the median (50% probability) dates of the
//! first autumn and last spring frost (32 °F) and whether a hard freeze
//! (28 °F) happens in a typical year. How it is built, from which public
//! domain sources, is in `docs/DATA-SOURCES.md`; the builder is
//! `tools/climate-build`.
//!
//! Pure: the lookup is a binary search over bytes compiled into the program.
//!
//! ## Binary format (version 1)
//!
//! ```text
//! "HWZC"  magic
//! u8      version (1)
//! u8      reserved (0)
//! u32 LE  record count
//! records, sorted by ZIP, 6 bytes each:
//!   u24 LE  ZIP as a number (00601 = 601)
//!   u24 LE  bits 0-8   first frost, non-leap day of year (0 = no frost in a typical year)
//!           bits 9-17  last frost, non-leap day of year (0 = none)
//!           bit  18    a hard freeze (28 °F) happens in a typical year
//!           bit  19    southern hemisphere
//! ```

use serde::{Deserialize, Serialize};

use crate::schedule::{Climate, Hemisphere};

/// The bundled table.
pub static US_ZIP_TABLE: &[u8] = include_bytes!("../data/us-zip-climate.bin");

pub const MAGIC: &[u8; 4] = b"HWZC";
pub const FORMAT_VERSION: u8 = 1;
const HEADER: usize = 10;
const RECORD: usize = 6;

/// One row of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZipClimate {
    pub zip: u32,
    pub first_frost_doy: Option<u16>,
    pub last_frost_doy: Option<u16>,
    pub hard_freeze: bool,
    pub south: bool,
}

impl ZipClimate {
    pub fn encode(&self) -> [u8; RECORD] {
        let z = self.zip.to_le_bytes();
        let packed: u32 = u32::from(self.first_frost_doy.unwrap_or(0) & 0x1ff)
            | (u32::from(self.last_frost_doy.unwrap_or(0) & 0x1ff) << 9)
            | (u32::from(self.hard_freeze) << 18)
            | (u32::from(self.south) << 19);
        let p = packed.to_le_bytes();
        [z[0], z[1], z[2], p[0], p[1], p[2]]
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < RECORD {
            return None;
        }
        let zip = u32::from_le_bytes([b[0], b[1], b[2], 0]);
        let packed = u32::from_le_bytes([b[3], b[4], b[5], 0]);
        let doy = |v: u32| match (v & 0x1ff) as u16 {
            0 => None,
            d => Some(d),
        };
        Some(Self {
            zip,
            first_frost_doy: doy(packed),
            last_frost_doy: doy(packed >> 9),
            hard_freeze: packed & (1 << 18) != 0,
            south: packed & (1 << 19) != 0,
        })
    }
}

/// Write a whole table (used by `tools/climate-build`). Rows must be sorted
/// by ZIP and unique.
pub fn encode_table(rows: &[ZipClimate]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + rows.len() * RECORD);
    out.extend_from_slice(MAGIC);
    out.push(FORMAT_VERSION);
    out.push(0);
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for r in rows {
        out.extend_from_slice(&r.encode());
    }
    out
}

/// A read-only view over an encoded table.
#[derive(Clone, Copy, Debug)]
pub struct ZipTable<'a> {
    records: &'a [u8],
}

impl<'a> ZipTable<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < HEADER || &bytes[..4] != MAGIC || bytes[4] != FORMAT_VERSION {
            return None;
        }
        let n = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
        let records = bytes.get(HEADER..HEADER + n * RECORD)?;
        Some(Self { records })
    }

    /// The bundled table.
    pub fn bundled() -> ZipTable<'static> {
        ZipTable::parse(US_ZIP_TABLE).expect("bundled climate table is valid")
    }

    pub fn len(&self) -> usize {
        self.records.len() / RECORD
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn row(&self, i: usize) -> Option<ZipClimate> {
        ZipClimate::decode(self.records.get(i * RECORD..(i + 1) * RECORD)?)
    }

    fn zip_at(&self, i: usize) -> u32 {
        let b = &self.records[i * RECORD..];
        u32::from_le_bytes([b[0], b[1], b[2], 0])
    }

    /// Look a ZIP code up. A ZIP that is not a ZCTA (many PO-box and
    /// single-building ZIPs are not) gets the numerically nearest ZCTA with
    /// the same first three digits, marked approximate.
    pub fn lookup(&self, zip: &str) -> Option<ClimateLookup> {
        let z = parse_zip(zip)?;
        let n = self.len();
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.zip_at(mid) < z {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < n && self.zip_at(lo) == z {
            return Some(ClimateLookup::from_row(zip, self.row(lo)?, false));
        }
        let prefix = z / 100;
        let below = lo
            .checked_sub(1)
            .filter(|&i| self.zip_at(i) / 100 == prefix);
        let above = Some(lo).filter(|&i| i < n && self.zip_at(i) / 100 == prefix);
        let pick = match (below, above) {
            (Some(b), Some(a)) => {
                if z - self.zip_at(b) <= self.zip_at(a) - z {
                    b
                } else {
                    a
                }
            }
            (Some(b), None) => b,
            (None, Some(a)) => a,
            (None, None) => return None,
        };
        Some(ClimateLookup::from_row(zip, self.row(pick)?, true))
    }
}

/// Five digits, optionally followed by "-1234".
pub fn parse_zip(s: &str) -> Option<u32> {
    let s = s.trim();
    let five = s.get(..5)?;
    let rest = &s[5..];
    if !five.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !(rest.is_empty()
        || (rest.len() == 5
            && rest.starts_with('-')
            && rest[1..].chars().all(|c| c.is_ascii_digit())))
    {
        return None;
    }
    five.parse().ok()
}

/// The answer for one ZIP code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClimateLookup {
    /// The ZIP as given (five digits).
    pub zip: String,
    /// The ZCTA the values come from (differs when approximate).
    pub matched_zip: String,
    pub approximate: bool,
    pub climate: Climate,
}

impl ClimateLookup {
    fn from_row(zip: &str, r: ZipClimate, approximate: bool) -> Self {
        Self {
            zip: zip.trim().chars().take(5).collect(),
            matched_zip: format!("{:05}", r.zip),
            approximate,
            climate: Climate {
                hemisphere: if r.south {
                    Hemisphere::South
                } else {
                    Hemisphere::North
                },
                first_frost_doy: r.first_frost_doy,
                last_frost_doy: r.last_frost_doy,
                hard_freeze: Some(r.hard_freeze),
            },
        }
    }
}

/// Look up a ZIP code in the bundled table.
pub fn lookup_us_zip(zip: &str) -> Option<ClimateLookup> {
    ZipTable::bundled().lookup(zip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let r = ZipClimate {
            zip: 99_950,
            first_frost_doy: Some(365),
            last_frost_doy: Some(1),
            hard_freeze: true,
            south: true,
        };
        assert_eq!(ZipClimate::decode(&r.encode()), Some(r));
        let none = ZipClimate {
            zip: 601,
            first_frost_doy: None,
            last_frost_doy: None,
            hard_freeze: false,
            south: false,
        };
        assert_eq!(ZipClimate::decode(&none.encode()), Some(none));
    }

    #[test]
    fn small_table_lookup_and_fallback() {
        let row = |zip, f| ZipClimate {
            zip,
            first_frost_doy: Some(f),
            last_frost_doy: Some(90),
            hard_freeze: true,
            south: false,
        };
        let bytes = encode_table(&[row(19103, 300), row(19107, 310), row(19200, 320)]);
        let t = ZipTable::parse(&bytes).unwrap();
        assert_eq!(t.len(), 3);
        let exact = t.lookup("19103").unwrap();
        assert!(!exact.approximate);
        assert_eq!(exact.climate.first_frost_doy, Some(300));
        let near = t.lookup("19106-1234").unwrap();
        assert!(near.approximate);
        assert_eq!(near.matched_zip, "19107");
        assert_eq!(t.lookup("19199").unwrap().matched_zip, "19107");
        assert!(t.lookup("20000").is_none());
        assert!(t.lookup("1910").is_none());
        assert!(t.lookup("abcde").is_none());
        assert!(ZipTable::parse(b"nope").is_none());
    }

    #[test]
    fn parse_zip_forms() {
        assert_eq!(parse_zip("19103"), Some(19103));
        assert_eq!(parse_zip(" 00601 "), Some(601));
        assert_eq!(parse_zip("19103-4321"), Some(19103));
        assert_eq!(parse_zip("191034"), None);
        assert_eq!(parse_zip("19103-12"), None);
    }
}
