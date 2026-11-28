//! Build `crates/core/data/us-zip-climate.bin` from public domain US
//! government data. See `docs/DATA-SOURCES.md` for where to get the inputs,
//! their checksums and licences.
//!
//! ```sh
//! cargo run -p housewright-climate-build --release -- \
//!     <normals dir> <2020_Gaz_zcta_national.txt> crates/core/data/us-zip-climate.bin
//! ```
//!
//! * `<normals dir>`: the extracted NOAA NCEI "U.S. Climate Normals 1991-2020,
//!   annual/seasonal, by station" archive: one CSV per station.
//! * The Census Bureau 2020 ZCTA Gazetteer file (tab separated): each ZCTA's
//!   internal point (latitude, longitude).
//!
//! For every ZCTA the nearest usable temperature station (great-circle
//! distance) supplies:
//! * first frost: `ANN-TMIN-PRBFST-T32FP50`, the date by which the first
//!   32 °F night has happened in half of years;
//! * last frost: `ANN-TMIN-PRBLST-T32FP50`;
//! * hard freeze: whether `ANN-TMIN-PRBFST-T28FP50` exists (a 28 °F night
//!   happens in at least half of years).
//!
//! A station is usable when it has temperature normals
//! (`ANN-TMIN-AVGNDS-LSTH032`). When its 32 °F dates are missing because
//! freezes are too rare to compute (flag `Y`) and it averages at most one
//! night a year at or below 32 °F, it counts as frost-free. A station whose
//! dates are missing for any other reason is skipped. Flag `V` (frost risk
//! all year) gives no dates and a hard freeze.
//!
//! Plain Rust, no dependencies beyond the core (which owns the file format).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use housewright_core::climate::{ZipClimate, encode_table};

#[derive(Clone, Debug)]
struct Station {
    id: String,
    lat: f64,
    lon: f64,
    first: Option<u16>,
    last: Option<u16>,
    hard_freeze: bool,
}

/// Split one CSV line with double-quoted fields.
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

/// "MM/DD" to a non-leap day of year (Feb 29 counts as Feb 28).
fn mmdd_to_doy(s: &str) -> Option<u16> {
    let (m, d) = s.trim().split_once('/')?;
    let m: usize = m.parse().ok()?;
    let d: u16 = d.parse().ok()?;
    const DAYS: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(1..=12).contains(&m) || d == 0 || d > 29 + u16::from(m != 2) * 2 {
        return None;
    }
    let d = d.min(DAYS[m - 1]);
    Some(DAYS[..m - 1].iter().sum::<u16>() + d)
}

enum Value {
    Date(u16),
    /// Too rare to compute.
    Rare,
    /// Frost risk all year.
    AllYear,
    Missing,
}

fn value(row: &HashMap<&str, &str>, col: &str) -> Value {
    let v = row.get(col).copied().unwrap_or("");
    let flag = row
        .get(format!("meas_flag_{col}").as_str())
        .copied()
        .unwrap_or("");
    if let Some(d) = mmdd_to_doy(v) {
        return Value::Date(d);
    }
    match flag {
        "Y" => Value::Rare,
        "V" => Value::AllYear,
        _ => Value::Missing,
    }
}

fn read_station(path: &Path) -> Option<Station> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let header = split_csv(lines.next()?);
    let data = split_csv(lines.next()?);
    let row: HashMap<&str, &str> = header
        .iter()
        .map(String::as_str)
        .zip(data.iter().map(String::as_str))
        .collect();
    let nights: f64 = row.get("ANN-TMIN-AVGNDS-LSTH032")?.parse().ok()?;
    if nights < 0.0 {
        return None; // -9999: no temperature normals
    }
    let lat: f64 = row.get("LATITUDE")?.parse().ok()?;
    let lon: f64 = row.get("LONGITUDE")?.parse().ok()?;
    let id = row.get("STATION")?.to_string();
    let first = value(&row, "ANN-TMIN-PRBFST-T32FP50");
    let last = value(&row, "ANN-TMIN-PRBLST-T32FP50");
    let hard = value(&row, "ANN-TMIN-PRBFST-T28FP50");
    let (first, last) = match (first, last) {
        (Value::Date(f), Value::Date(l)) => (Some(f), Some(l)),
        (Value::Rare, Value::Rare) if nights <= 1.0 => (None, None),
        (Value::AllYear, _) | (_, Value::AllYear) => (None, None),
        _ => return None,
    };
    let hard_freeze = match hard {
        Value::Date(_) | Value::AllYear => true,
        Value::Rare => false,
        Value::Missing => return None,
    };
    Some(Station {
        id,
        lat,
        lon,
        first,
        last,
        hard_freeze,
    })
}

fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1) = (a.0.to_radians(), a.1.to_radians());
    let (la2, lo2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: climate-build <normals dir> <zcta gazetteer txt> <out.bin>");
        std::process::exit(2);
    }
    let normals = PathBuf::from(&args[1]);
    let gazetteer = PathBuf::from(&args[2]);
    let out = PathBuf::from(&args[3]);

    let mut files: Vec<PathBuf> = std::fs::read_dir(&normals)
        .expect("normals directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();
    let stations: Vec<Station> = files.iter().filter_map(|p| read_station(p)).collect();
    let frost_free = stations.iter().filter(|s| s.first.is_none()).count();
    eprintln!(
        "{} station files, {} usable temperature stations ({} frost-free)",
        files.len(),
        stations.len(),
        frost_free
    );

    let gaz = std::fs::read_to_string(&gazetteer).expect("gazetteer file");
    let mut lines = gaz.lines();
    let header: Vec<&str> = lines
        .next()
        .expect("header")
        .split('\t')
        .map(str::trim)
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| panic!("gazetteer has no {name} column"))
    };
    let (c_id, c_lat, c_lon) = (col("GEOID"), col("INTPTLAT"), col("INTPTLONG"));

    let mut rows = Vec::new();
    let mut far = 0usize;
    let mut max_km: f64 = 0.0;
    let mut used: HashMap<&str, usize> = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').map(str::trim).collect();
        let (Some(id), Some(lat), Some(lon)) = (f.get(c_id), f.get(c_lat), f.get(c_lon)) else {
            continue;
        };
        let (Ok(zip), Ok(lat), Ok(lon)) =
            (id.parse::<u32>(), lat.parse::<f64>(), lon.parse::<f64>())
        else {
            continue;
        };
        let (best, km) = stations
            .iter()
            .map(|s| (s, distance_km((lat, lon), (s.lat, s.lon))))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("at least one station");
        if km > 100.0 {
            far += 1;
        }
        max_km = max_km.max(km);
        *used.entry(best.id.as_str()).or_default() += 1;
        rows.push(ZipClimate {
            zip,
            first_frost_doy: best.first,
            last_frost_doy: best.last,
            hard_freeze: best.hard_freeze,
            south: lat < 0.0,
        });
    }
    rows.sort_by_key(|r| r.zip);
    rows.dedup_by_key(|r| r.zip);
    let bytes = encode_table(&rows);
    std::fs::write(&out, &bytes).expect("write output");
    eprintln!(
        "{} ZCTAs from {} stations; {} more than 100 km from a station (max {:.0} km); {} frost-free; {} bytes",
        rows.len(),
        used.len(),
        far,
        max_km,
        rows.iter().filter(|r| r.first_frost_doy.is_none()).count(),
        bytes.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_and_dates() {
        assert_eq!(
            split_csv(r#""A","B, C",  3 ,"say ""hi""""#),
            ["A", "B, C", "3", r#"say "hi""#]
        );
        assert_eq!(mmdd_to_doy("01/01"), Some(1));
        assert_eq!(mmdd_to_doy("11/10"), Some(314));
        assert_eq!(mmdd_to_doy("02/29"), Some(59));
        assert_eq!(mmdd_to_doy("12/31"), Some(365));
        assert_eq!(mmdd_to_doy("-9999.0"), None);
        assert_eq!(mmdd_to_doy("13/01"), None);
    }

    #[test]
    fn distance() {
        let philly = (39.95, -75.17);
        let nyc = (40.78, -73.97);
        let d = distance_km(philly, nyc);
        assert!((d - 135.0).abs() < 10.0, "{d}");
    }
}
