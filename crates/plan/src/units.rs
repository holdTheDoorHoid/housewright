//! Lengths and areas as people write them: `12' 6"`, `12 ft 6 in`,
//! `3.81 m`, `381 cm`. Stored values are always whole millimetres; the
//! household's setting (D-06) picks how they are shown.

use serde::{Deserialize, Serialize};

/// How lengths are shown and how a bare number is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    /// Feet and inches, square feet (the US default, D-06).
    #[default]
    Imperial,
    /// Metres, square metres.
    Metric,
}

pub const MM_PER_INCH: f64 = 25.4;

/// A length for people: `12' 6"` (to the nearest inch) or `3.81 m` (to the
/// centimetre).
pub fn format_length(mm: i64, units: Units) -> String {
    let neg = mm < 0;
    let mm = mm.unsigned_abs() as f64;
    let s = match units {
        Units::Imperial => {
            let inches = (mm / MM_PER_INCH).round() as u64;
            let (ft, inch) = (inches / 12, inches % 12);
            if ft == 0 {
                format!("{inch}\"")
            } else {
                format!("{ft}' {inch}\"")
            }
        }
        Units::Metric => format!("{:.2} m", mm / 1000.0),
    };
    if neg { format!("-{s}") } else { s }
}

/// An area for people: `152 sq ft` or `14.1 m²`.
pub fn format_area(mm2: f64, units: Units) -> String {
    match units {
        Units::Imperial => {
            let sqft = mm2 / (MM_PER_INCH * 12.0).powi(2);
            format!("{} sq ft", group(sqft.round() as i64))
        }
        Units::Metric => format!("{:.1} m²", mm2 / 1_000_000.0),
    }
}

fn group(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Why a typed length could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Ft,
    In,
    M,
    Cm,
    Mm,
}

impl Unit {
    fn mm(self) -> f64 {
        match self {
            Unit::Ft => MM_PER_INCH * 12.0,
            Unit::In => MM_PER_INCH,
            Unit::M => 1000.0,
            Unit::Cm => 10.0,
            Unit::Mm => 1.0,
        }
    }
    fn metric(self) -> bool {
        matches!(self, Unit::M | Unit::Cm | Unit::Mm)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tok {
    Num(f64, bool),
    Unit(Unit),
}

fn unit_word(w: &str) -> Option<Unit> {
    Some(match w {
        "'" | "’" | "′" | "ft" | "foot" | "feet" => Unit::Ft,
        "\"" | "”" | "″" | "''" | "in" | "inch" | "inches" => Unit::In,
        "m" | "meter" | "meters" | "metre" | "metres" => Unit::M,
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => Unit::Cm,
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Unit::Mm,
        _ => return None,
    })
}

fn lex(s: &str) -> Result<Vec<Tok>, ParseError> {
    let bad = || ParseError(format!("could not read \"{}\" as a length", s.trim()));
    let mut out = Vec::new();
    let chars: Vec<char> = s.trim().to_lowercase().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let a: String = chars[start..i].iter().collect();
            let a: f64 = a.parse().map_err(|_| bad())?;
            if i < chars.len() && chars[i] == '/' {
                i += 1;
                let s2 = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let b: String = chars[s2..i].iter().collect();
                let b: f64 = b.parse().map_err(|_| bad())?;
                if b == 0.0 {
                    return Err(bad());
                }
                out.push(Tok::Num(a / b, true));
            } else {
                out.push(Tok::Num(a, false));
            }
        } else if let Some(v) = vulgar(c) {
            out.push(Tok::Num(v, true));
            i += 1;
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            let w: String = chars[start..i].iter().collect();
            out.push(Tok::Unit(unit_word(&w).ok_or_else(bad)?));
        } else {
            // Quote marks; two single quotes mean inches.
            let w: String = if c == '\'' && chars.get(i + 1) == Some(&'\'') {
                i += 2;
                "''".into()
            } else {
                i += 1;
                c.to_string()
            };
            out.push(Tok::Unit(unit_word(&w).ok_or_else(bad)?));
        }
    }
    // Join a whole number and a following fraction: "6 1/2" or "6½".
    let mut joined: Vec<Tok> = Vec::new();
    for t in out {
        match (joined.last_mut(), t) {
            (Some(Tok::Num(a, false)), Tok::Num(b, true)) if a.fract() == 0.0 => {
                *a += b;
                if let Some(Tok::Num(_, f)) = joined.last_mut() {
                    *f = true;
                }
            }
            (_, t) => joined.push(t),
        }
    }
    Ok(joined)
}

fn vulgar(c: char) -> Option<f64> {
    Some(match c {
        '½' => 0.5,
        '¼' => 0.25,
        '¾' => 0.75,
        '⅛' => 0.125,
        '⅜' => 0.375,
        '⅝' => 0.625,
        '⅞' => 0.875,
        _ => return None,
    })
}

/// The longest length accepted (one kilometre).
pub const MAX_LENGTH_MM: i64 = 1_000_000;

/// Read a typed length into whole millimetres.
///
/// Accepts `12' 6"`, `12'6`, `12 ft 6 in`, `12 6` (feet and inches when
/// `default` is imperial), `6 1/2"`, `150"`, `12.5 ft`, `3.81 m`,
/// `3 m 20 cm`, `381 cm`, `3810 mm`. A bare number is feet (imperial) or
/// metres (metric). A number after feet with no unit is inches; after
/// metres, centimetres. Feet and metres cannot be mixed.
pub fn parse_length(s: &str, default: Units) -> Result<i64, ParseError> {
    let toks = lex(s)?;
    if toks.is_empty() {
        return Err(ParseError("type a length, like 12' 6\" or 3.81 m".into()));
    }
    let bad = || ParseError(format!("could not read \"{}\" as a length", s.trim()));
    let mut total = 0.0;
    let mut last: Option<Unit> = None;
    let mut system: Option<bool> = None;
    let mut i = 0;
    while i < toks.len() {
        let Tok::Num(v, _) = toks[i] else {
            return Err(bad());
        };
        let unit = match toks.get(i + 1) {
            Some(Tok::Unit(u)) => {
                i += 2;
                *u
            }
            _ => {
                i += 1;
                match last {
                    None => match default {
                        Units::Imperial => Unit::Ft,
                        Units::Metric => Unit::M,
                    },
                    Some(Unit::Ft) => Unit::In,
                    Some(Unit::M) => Unit::Cm,
                    Some(Unit::Cm) => Unit::Mm,
                    Some(_) => return Err(bad()),
                }
            }
        };
        // Bigger units first: 12' 6", never 6" 12'.
        if let Some(l) = last
            && unit.mm() >= l.mm()
        {
            return Err(bad());
        }
        match system {
            Some(m) if m != unit.metric() => {
                return Err(ParseError(
                    "use either feet and inches or metres, not both".into(),
                ));
            }
            _ => system = Some(unit.metric()),
        }
        total += v * unit.mm();
        last = Some(unit);
    }
    let mm = total.round();
    if mm > MAX_LENGTH_MM as f64 {
        return Err(ParseError("that is longer than a kilometre".into()));
    }
    Ok(mm as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(format_length(3810, Units::Imperial), "12' 6\"");
        assert_eq!(format_length(3048, Units::Imperial), "10' 0\"");
        assert_eq!(format_length(152, Units::Imperial), "6\"");
        assert_eq!(format_length(3810, Units::Metric), "3.81 m");
        assert_eq!(format_length(-1000, Units::Metric), "-1.00 m");
        // 11' 11.8" rounds up to 12' 0", never 11' 12".
        assert_eq!(format_length(3652, Units::Imperial), "12' 0\"");
        assert_eq!(format_area(12_000_000.0, Units::Metric), "12.0 m²");
        assert_eq!(format_area(12_000_000.0, Units::Imperial), "129 sq ft");
        assert_eq!(
            format_area(1_000_000_000.0, Units::Imperial),
            "10,764 sq ft"
        );
    }

    #[test]
    fn parsing() {
        let i = Units::Imperial;
        let m = Units::Metric;
        for (s, u, want) in [
            ("12' 6\"", i, 3810),
            ("12'6\"", i, 3810),
            ("12'6", i, 3810),
            ("12 ft 6 in", i, 3810),
            ("12 feet 6 inches", i, 3810),
            ("12 6", i, 3810),
            ("12’ 6”", i, 3810),
            ("12′ 6″", i, 3810),
            ("12", i, 3658),
            ("12.5 ft", i, 3810),
            ("150\"", i, 3810),
            ("150 in", i, 3810),
            ("6 1/2\"", i, 165),
            ("6½\"", i, 165),
            ("2' 6 1/2\"", i, 775),
            ("1/2\"", i, 13),
            ("3.81 m", i, 3810),
            ("3.81", m, 3810),
            ("3.81m", m, 3810),
            ("381 cm", m, 3810),
            ("3810 mm", m, 3810),
            ("3 m 81 cm", m, 3810),
            ("3 m 81", m, 3810),
            ("  3.81 M ", m, 3810),
            ("0", m, 0),
        ] {
            assert_eq!(parse_length(s, u), Ok(want), "{s}");
        }
        for s in [
            "", "abc", "12' 6' ", "6\" 12'", "3 m 2 ft", "1/0", "12 6 7", "2000 m", "12''' ",
        ] {
            assert!(parse_length(s, i).is_err(), "{s}");
        }
        assert_eq!(
            parse_length("12'' ", i),
            Ok(305),
            "two single quotes are inches"
        );
    }
}
