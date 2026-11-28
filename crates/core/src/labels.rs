//! Labels on things: the short human-readable code and QR link printed on a
//! sticker (D-30), and reading brand, model and serial from the text of an
//! appliance rating plate (D-29).
//!
//! Everything here is pure: text in, text out. Reading pixels (QR decoding,
//! OCR) is `housewright-media`'s job.

use sha2::{Digest, Sha256};

use crate::ids::ItemId;

/// Crockford base 32: no I, L, O or U, so a code survives being read aloud
/// or retyped.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Characters after "HW-". Five give 33 million codes: two things in one
/// home sharing a code is very unlikely, and [`crate`] users must still
/// handle it (a lookup can return more than one thing).
pub const CODE_LEN: usize = 5;
/// The URL scheme a sticker's QR code carries.
pub const SCHEME: &str = "housewright://item/";

/// The sticker code for an item, e.g. `HW-7K3QM`. Derived from the id, so
/// every device computes the same code with nothing stored or synced.
pub fn short_code(item: ItemId) -> String {
    let digest = Sha256::digest(item.to_string().as_bytes());
    let mut bits: u64 = 0;
    for b in &digest[..5] {
        bits = (bits << 8) | u64::from(*b);
    }
    let mut out = String::from("HW-");
    for i in 0..CODE_LEN {
        let shift = 40 - 5 * (i + 1);
        out.push(ALPHABET[((bits >> shift) & 31) as usize] as char);
    }
    out
}

/// Normalise something a person typed ("hw 7k3qm", "7K3QM", "HW-7K3OM")
/// to the canonical code, or `None` if it cannot be one.
pub fn normalise_code(input: &str) -> Option<String> {
    let mut s: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if s.len() == CODE_LEN + 2 && s.starts_with("HW") {
        s.drain(..2);
    }
    if s.len() != CODE_LEN {
        return None;
    }
    let mapped: String = s
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            'U' => 'V',
            c => c,
        })
        .collect();
    mapped
        .bytes()
        .all(|b| ALPHABET.contains(&b))
        .then(|| format!("HW-{mapped}"))
}

/// The QR content for an item: `housewright://item/<id>`.
pub fn item_url(item: ItemId) -> String {
    format!("{SCHEME}{item}")
}

/// What a scanned QR code or typed text points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelTarget {
    Item(ItemId),
    Code(String),
}

/// Read a sticker's content: our URL, or a short code on its own.
pub fn parse_label(text: &str) -> Option<LabelTarget> {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix(SCHEME) {
        let id = rest.split(['?', '#', '/']).next().unwrap_or_default();
        return id.parse().ok().map(LabelTarget::Item);
    }
    normalise_code(t).map(LabelTarget::Code)
}

// -- rating plates --------------------------------------------------------

/// Appliance and equipment makers found on US rating plates. Names only
/// (facts, not manufacturer content); longer names first so "A.O. Smith"
/// wins over "Smith".
pub const KNOWN_BRANDS: &[&str] = &[
    "Bradford White",
    "General Electric",
    "A.O. Smith",
    "Speed Queen",
    "Weil-McLain",
    "InSinkErator",
    "First Alert",
    "KitchenAid",
    "LiftMaster",
    "Chamberlain",
    "Frigidaire",
    "Electrolux",
    "Mitsubishi",
    "Viessmann",
    "Honeywell",
    "Thermador",
    "Whirlpool",
    "Aprilaire",
    "Craftsman",
    "Hotpoint",
    "Sub-Zero",
    "Culligan",
    "Fujitsu",
    "Generac",
    "Goodman",
    "Burnham",
    "Pentair",
    "Hayward",
    "Samsung",
    "Kenmore",
    "Rinnai",
    "Navien",
    "Lennox",
    "Carrier",
    "Zoeller",
    "Daikin",
    "Bryant",
    "Maytag",
    "Kohler",
    "Rheem",
    "Trane",
    "Amana",
    "Bosch",
    "Broan",
    "Kidde",
    "Haier",
    "Miele",
    "Genie",
    "Honda",
    "Ruud",
    "York",
    "Toro",
    "LG",
    "GE",
];

/// Words on a plate that say what kind of thing it is, and the bundled
/// template for it. Checked in order; the first match wins.
pub const KIND_WORDS: &[(&str, &str)] = &[
    ("HEAT PUMP WATER HEATER", "heat-pump-water-heater"),
    ("HYBRID WATER HEATER", "heat-pump-water-heater"),
    ("TANKLESS", "tankless-water-heater"),
    ("WATER HEATER", "water-heater"),
    ("BOILER", "boiler"),
    ("FURNACE", "furnace"),
    ("HEAT PUMP", "heat-pump"),
    ("CONDENSING UNIT", "central-ac"),
    ("AIR CONDITIONER", "central-ac"),
    ("DEHUMIDIFIER", "dehumidifier"),
    ("DISHWASHER", "dishwasher"),
    ("REFRIGERATOR", "refrigerator"),
    ("FREEZER", "refrigerator"),
    ("DRYER", "clothes-dryer"),
    ("WASHER", "washing-machine"),
    ("WATER SOFTENER", "water-softener"),
    ("SUMP PUMP", "sump-pump"),
    ("DISPOSER", "garbage-disposal"),
    ("GENERATOR", "generator"),
    ("GARAGE DOOR OPENER", "garage-door-opener"),
    ("RANGE HOOD", "range-hood"),
    ("SMOKE ALARM", "smoke-alarm"),
    ("CARBON MONOXIDE", "co-alarm"),
];

/// What was read from a rating plate. Every field is only a candidate: the
/// person always sees it and can correct it, and nothing is saved until
/// they say so.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PlateReading {
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    /// Other possible model or serial values, best first.
    pub model_alternatives: Vec<String>,
    pub serial_alternatives: Vec<String>,
    /// Bundled template that matches the words on the plate.
    pub template: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tag {
    Model,
    Serial,
    /// Any other "LABEL:" (VOLTS, AMPS, MFD); it ends a value.
    Other,
}

/// Labels that introduce a value, as normalised words (uppercase, no
/// punctuation). Longest first.
const TAGS: &[(&[&str], Tag)] = &[
    (&["MODEL", "NUMBER"], Tag::Model),
    (&["MODEL", "NO"], Tag::Model),
    (&["MOD", "NO"], Tag::Model),
    (&["SERIAL", "NUMBER"], Tag::Serial),
    (&["SERIAL", "NO"], Tag::Serial),
    (&["SER", "NO"], Tag::Serial),
    (&["MODEL"], Tag::Model),
    (&["MOD"], Tag::Model),
    (&["MDL"], Tag::Model),
    (&["MN"], Tag::Model),
    (&["M", "N"], Tag::Model),
    (&["SERIAL"], Tag::Serial),
    (&["SER"], Tag::Serial),
    (&["SN"], Tag::Serial),
    (&["S", "N"], Tag::Serial),
    (&["VOLTS"], Tag::Other),
    (&["VOLTAGE"], Tag::Other),
    (&["AMPS"], Tag::Other),
    (&["HZ"], Tag::Other),
    (&["PHASE"], Tag::Other),
    (&["MFD"], Tag::Other),
    (&["DATE"], Tag::Other),
    (&["TYPE"], Tag::Other),
    (&["INPUT"], Tag::Other),
    (&["CAPACITY"], Tag::Other),
    (&["PSI"], Tag::Other),
    (&["BTU"], Tag::Other),
    (&["WATTS"], Tag::Other),
    (&["MADE"], Tag::Other),
    (&["LISTED"], Tag::Other),
];

#[derive(Debug)]
struct Word {
    /// As printed, punctuation inside kept (`WM-4000/HW`).
    raw: String,
    /// Uppercase letters and digits only, for matching labels.
    norm: String,
    /// The word ended with a colon (or something OCR reads for one).
    colon: bool,
}

fn words(line: &str) -> Vec<Word> {
    // "M/N" and "S/N" are labels, not values: split them so they match.
    line.split_whitespace()
        .flat_map(|w| {
            let up = w.to_ascii_uppercase();
            let bare = up.trim_end_matches([':', ';', '.', '#']);
            if bare == "M/N" || bare == "S/N" {
                vec![
                    w[..1].to_string(),
                    format!("{}{}", &w[2..3], &w[3.min(w.len())..]),
                ]
            } else if let Some((a, b)) = w.split_once([':', ';']).filter(|(_, b)| !b.is_empty()) {
                // "MODEL:ABC123" -> "MODEL:" "ABC123"
                vec![format!("{a}:"), b.to_string()]
            } else {
                vec![w.to_string()]
            }
        })
        .map(|w| {
            let colon = w.ends_with([':', ';']);
            let raw = w
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && !"-/.".contains(c))
                .trim_matches(['.', '/'])
                .to_string();
            let norm = raw
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_uppercase())
                .collect();
            Word { raw, norm, colon }
        })
        .filter(|w| !w.raw.is_empty() || w.colon)
        .collect()
}

/// Does a tag start at `words[i]`? Returns the tag and how many words it uses.
fn tag_at(ws: &[Word], i: usize) -> Option<(Tag, usize)> {
    for (pat, tag) in TAGS {
        if i + pat.len() > ws.len() {
            continue;
        }
        if pat.iter().enumerate().all(|(k, p)| ws[i + k].norm == *p) {
            let mut n = pat.len();
            // Swallow a following "NO." / "#" / "NUMBER".
            while i + n < ws.len() && matches!(ws[i + n].norm.as_str(), "NO" | "" | "NUMBER") {
                if ws[i + n].norm.is_empty() && !ws[i + n].colon {
                    break;
                }
                n += 1;
            }
            // A one- or two-letter tag ("SN", "M N") needs a colon or must
            // start the line, or "SN" inside a value would count.
            let short = pat.iter().map(|p| p.len()).sum::<usize>() <= 2;
            if short && !(ws[i + n - 1].colon || i == 0) {
                continue;
            }
            return Some((*tag, n));
        }
    }
    None
}

/// A plausible model or serial: 3 to 30 characters, at least one digit,
/// mostly letters and digits.
fn plausible(v: &str) -> bool {
    let n = v.chars().count();
    (3..=30).contains(&n)
        && v.chars().any(|c| c.is_ascii_digit())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-/.".contains(c))
}

/// Pull brand, model and serial candidates out of OCR lines.
pub fn read_plate(lines: &[String]) -> PlateReading {
    let mut models: Vec<String> = Vec::new();
    let mut serials: Vec<String> = Vec::new();
    let all: Vec<Vec<Word>> = lines.iter().map(|l| words(l)).collect();
    for (li, ws) in all.iter().enumerate() {
        let mut i = 0;
        while i < ws.len() {
            let Some((tag, n)) = tag_at(ws, i) else {
                i += 1;
                continue;
            };
            i += n;
            if tag == Tag::Other {
                continue;
            }
            // The value: words up to the next tag on this line; if none,
            // the first word of the next line.
            let mut value: Vec<&str> = Vec::new();
            let mut j = i;
            while j < ws.len() && tag_at(ws, j).is_none() {
                if !ws[j].raw.is_empty() {
                    value.push(&ws[j].raw);
                }
                j += 1;
            }
            if value.is_empty()
                && let Some(next) = all.get(li + 1)
                && let Some(first) = next.first()
                && tag_at(next, 0).is_none()
            {
                value.push(&first.raw);
            }
            // A value is one token; a two-token value joined by nothing
            // ("WM 4000") is common, so try the joined form when the first
            // token alone is not plausible.
            let cand = match value.as_slice() {
                [] => None,
                [a, ..] if plausible(a) => Some(a.to_string()),
                [a, b, ..] if plausible(&format!("{a}{b}")) => Some(format!("{a}{b}")),
                _ => None,
            };
            if let Some(c) = cand {
                let list = if tag == Tag::Model {
                    &mut models
                } else {
                    &mut serials
                };
                if !list.contains(&c) {
                    list.push(c.to_ascii_uppercase());
                }
            }
            i = j;
        }
    }
    let text = lines.join(" \n ");
    let upper = text.to_ascii_uppercase();
    let brand = KNOWN_BRANDS
        .iter()
        .find(|b| contains_word(&upper, &b.to_ascii_uppercase()))
        .map(|b| b.to_string());
    let template = KIND_WORDS
        .iter()
        .find(|(w, _)| contains_word(&upper, w))
        .map(|(_, t)| t.to_string());
    PlateReading {
        brand,
        model: models.first().cloned(),
        serial: serials.first().cloned(),
        model_alternatives: models.into_iter().skip(1).collect(),
        serial_alternatives: serials.into_iter().skip(1).collect(),
        template,
    }
}

/// `needle` appears in `hay` with no letter or digit on either side.
fn contains_word(hay: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = hay[from..].find(needle) {
        let start = from + pos;
        let end = start + needle.len();
        let before = hay[..start].chars().next_back();
        let after = hay[end..].chars().next();
        let edge = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric());
        if edge(before) && edge(after) {
            return true;
        }
        from = start + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    }

    #[test]
    fn codes_are_stable_and_retypable() {
        let id: ItemId = "01a21fbc-5607-72ea-b419-dc88b45dcfbf".parse().unwrap();
        let c = short_code(id);
        assert_eq!(c, short_code(id));
        assert_eq!(c.len(), 3 + CODE_LEN);
        assert!(c.starts_with("HW-"));
        assert_eq!(normalise_code(&c).as_deref(), Some(c.as_str()));
        let typed = c[3..].to_lowercase();
        assert_eq!(normalise_code(&typed).as_deref(), Some(c.as_str()));
        assert_eq!(
            normalise_code(&format!("hw {typed}")).as_deref(),
            Some(c.as_str())
        );
        assert_eq!(normalise_code("HW-0O1IL").as_deref(), Some("HW-00111"));
        assert_eq!(normalise_code("toolong123"), None);
        assert_eq!(normalise_code(""), None);
    }

    #[test]
    fn codes_rarely_collide() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..2000 {
            seen.insert(short_code(ItemId::new()));
        }
        assert!(seen.len() >= 1998, "{} unique of 2000", seen.len());
    }

    #[test]
    fn urls_round_trip() {
        let id = ItemId::new();
        assert_eq!(parse_label(&item_url(id)), Some(LabelTarget::Item(id)));
        assert_eq!(
            parse_label(&format!("{}?from=sticker", item_url(id))),
            Some(LabelTarget::Item(id))
        );
        assert_eq!(
            parse_label(&short_code(id)),
            Some(LabelTarget::Code(short_code(id)))
        );
        assert_eq!(parse_label("housewright://item/nonsense"), None);
        assert_eq!(parse_label("https://example.com/"), None);
    }

    // Synthetic rating-plate text, written for these tests; not copied
    // from any manufacturer.

    #[test]
    fn water_heater_plate() {
        let r = read_plate(&lines(
            "RHEEM MANUFACTURING
             GAS WATER HEATER
             MODEL NO. XG40T06EC36U1
             SERIAL NO. Q081912345
             CAPACITY 40 GAL  INPUT 36000 BTU/HR",
        ));
        assert_eq!(r.brand.as_deref(), Some("Rheem"));
        assert_eq!(r.model.as_deref(), Some("XG40T06EC36U1"));
        assert_eq!(r.serial.as_deref(), Some("Q081912345"));
        assert_eq!(r.template.as_deref(), Some("water-heater"));
    }

    #[test]
    fn one_line_with_slashes_and_colons() {
        let r = read_plate(&lines(
            "Whirlpool Corporation  DISHWASHER
             M/N: WDT750SAKZ0  S/N: F12345678
             120V 60Hz 12A",
        ));
        assert_eq!(r.brand.as_deref(), Some("Whirlpool"));
        assert_eq!(r.model.as_deref(), Some("WDT750SAKZ0"));
        assert_eq!(r.serial.as_deref(), Some("F12345678"));
        assert_eq!(r.template.as_deref(), Some("dishwasher"));
    }

    #[test]
    fn value_on_the_next_line_and_ocr_semicolons() {
        let r = read_plate(&lines(
            "Furnace - Upflow
             MODEL;
             G96VTN0802
             SERIAL #
             5918A12345
             MFD: 2019-04",
        ));
        assert_eq!(r.model.as_deref(), Some("G96VTN0802"));
        assert_eq!(r.serial.as_deref(), Some("5918A12345"));
        assert_eq!(r.template.as_deref(), Some("furnace"));
        assert_eq!(r.brand, None, "no known maker on the plate");
    }

    #[test]
    fn compact_labels_and_two_models() {
        let r = read_plate(&lines(
            "LG Electronics
             MODEL:WM4000HWA
             SN 912KWZT0A123
             Also fits model WM4000HBA",
        ));
        assert_eq!(r.brand.as_deref(), Some("LG"));
        assert_eq!(r.model.as_deref(), Some("WM4000HWA"));
        assert_eq!(r.model_alternatives, vec!["WM4000HBA".to_string()]);
        assert_eq!(r.serial.as_deref(), Some("912KWZT0A123"));
    }

    #[test]
    fn brand_must_be_a_whole_word() {
        // "GE" inside "GENERATOR" and "VOLTAGE" is not the brand GE.
        let r = read_plate(&lines(
            "PORTABLE GENERATOR  VOLTAGE 120/240
             MODEL NUMBER: PG7500E
             SERIAL NUMBER: 3001234567",
        ));
        assert_eq!(r.brand, None);
        assert_eq!(r.template.as_deref(), Some("generator"));
        assert_eq!(r.model.as_deref(), Some("PG7500E"));
        assert_eq!(r.serial.as_deref(), Some("3001234567"));
    }

    #[test]
    fn nothing_readable() {
        let r = read_plate(&lines("CAUTION HOT SURFACE\nDO NOT TOUCH"));
        assert_eq!(r, PlateReading::default());
        assert_eq!(read_plate(&[]), PlateReading::default());
    }

    #[test]
    fn words_without_digits_are_not_models() {
        let r = read_plate(&lines("MODEL SEE BELOW\nSERIAL NUMBER ON DOOR"));
        assert_eq!(r.model, None);
        assert_eq!(r.serial, None);
    }
}
