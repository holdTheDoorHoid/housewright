//! The bundled ZIP climate table answers sensibly for well-known places.
//! Expected values come from the NOAA 1991-2020 normals of the station the
//! builder picked; the ranges are loose so a rebuild with a newer edition of
//! the normals does not break them, but they catch a broken table.

use housewright_core::climate::{ZipTable, lookup_us_zip};
use housewright_core::schedule::Hemisphere;

fn doy(zip: &str) -> (u16, u16, bool) {
    let c = lookup_us_zip(zip)
        .unwrap_or_else(|| panic!("{zip} missing"))
        .climate;
    (
        c.first_frost_doy.expect("first frost"),
        c.last_frost_doy.expect("last frost"),
        c.freezes(),
    )
}

#[test]
fn table_is_complete_and_small() {
    let t = ZipTable::bundled();
    assert!(t.len() > 30_000, "{} ZCTAs", t.len());
    assert!(housewright_core::climate::US_ZIP_TABLE.len() < 300_000);
}

#[test]
fn philadelphia_has_autumn_frost_and_hard_freezes() {
    let (first, last, freezes) = doy("19103");
    // Early November to early December; mid March to mid April.
    assert!((305..=340).contains(&first), "{first}");
    assert!((74..=105).contains(&last), "{last}");
    assert!(freezes);
}

#[test]
fn minneapolis_freezes_earlier_than_philadelphia() {
    assert!(doy("55401").0 < doy("19103").0);
    assert!(doy("55401").1 > doy("19103").1);
}

#[test]
fn phoenix_has_no_hard_freeze() {
    let c = lookup_us_zip("85004").unwrap().climate;
    assert!(!c.freezes(), "{c:?}");
    assert_eq!(c.hemisphere, Hemisphere::North);
}

#[test]
fn miami_and_san_juan_are_frost_free() {
    for z in ["33130", "00901"] {
        let c = lookup_us_zip(z).unwrap().climate;
        assert_eq!(c.first_frost_doy, None, "{z}");
        assert!(!c.freezes(), "{z}");
    }
}

#[test]
fn american_samoa_is_southern_hemisphere() {
    let c = lookup_us_zip("96799").unwrap().climate;
    assert_eq!(c.hemisphere, Hemisphere::South);
}

#[test]
fn unknown_and_malformed_zips() {
    assert!(lookup_us_zip("00000").is_none());
    assert!(lookup_us_zip("hello").is_none());
    assert!(lookup_us_zip("").is_none());
    // A ZIP that is not a ZCTA falls back to a neighbour in the same prefix.
    let t = ZipTable::bundled();
    let first = t.row(0).unwrap().zip;
    let next = t.row(1).unwrap().zip;
    if next > first + 1 {
        let gap = format!("{:05}", first + 1);
        if (first + 1) / 100 == first / 100 {
            assert!(lookup_us_zip(&gap).unwrap().approximate);
        }
    }
}
