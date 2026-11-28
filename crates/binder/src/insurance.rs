//! The insurance inventory (D-27, Phase 9): every thing in the home, room by
//! room, with a photo, make, model, serial, when it was bought, what it
//! cost and is worth now, its receipts and its warranty, as one PDF (Typst,
//! like the binder) and one CSV spreadsheet.
//!
//! **Sensitive things and fields are left out unless asked for.** Insurers
//! do need the serial numbers of valuables, so the household can tick
//! "include sensitive items"; by default a thing marked sensitive (a safe,
//! jewellery) is left out entirely, and a serial, warranty reference,
//! photo or receipt marked sensitive is left out of the things that are
//! printed. The cover says how many were left out. Contractor details and
//! private fields are never part of an inventory.

use std::collections::BTreeMap;

use housewright_core::model::{AttachmentKind, Money, Sensitivity};
use housewright_core::warranty;

use crate::data::{HouseData, ItemData};
use crate::doc::{Block, Document, Edition, InventoryEntry, Section, Volume};
use crate::text;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InsuranceOptions {
    /// Include things and fields marked sensitive (default: no).
    pub include_sensitive: bool,
}

/// One line of the inventory, already filtered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub room: String,
    pub name: String,
    pub make: String,
    pub model: String,
    pub serial: String,
    pub purchased: String,
    pub installed: String,
    pub price: Option<Money>,
    pub value: Option<Money>,
    pub warranty: String,
    pub warranty_ends: String,
    pub receipts: Vec<String>,
    pub photos: usize,
    /// `img/<sha256>.jpg` for the printed thumbnail.
    pub photo: Option<String>,
    pub sensitive: bool,
}

/// The inventory's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub lines: Vec<Line>,
    /// Things left out because they are marked sensitive.
    pub withheld_items: usize,
    /// Serials, references, photos and receipts left out of printed things.
    pub withheld_details: usize,
    /// Totals in the household's currency (minor units); amounts in other
    /// currencies are counted, not converted.
    pub total_price: i64,
    pub total_value: i64,
    pub other_currency: usize,
    pub currency: String,
}

fn sens(s: Sensitivity) -> bool {
    s == Sensitivity::Sensitive
}

fn line(d: &HouseData, it: &ItemData, opts: InsuranceOptions, withheld: &mut usize) -> Line {
    let i = &it.item;
    let whole = sens(i.sensitivity);
    let mut serial = String::new();
    if let Some(s) = &i.serial {
        if sens(s.sensitivity) && !opts.include_sensitive {
            *withheld += 1;
        } else {
            serial = s.value.clone();
        }
    }
    let (mut w, mut ends) = (String::new(), String::new());
    if let Some(wr) = &i.warranty {
        let end = warranty::end(i).unwrap_or(wr.expires);
        ends = end.to_string();
        w = match warranty::state(end, d.today) {
            warranty::WarrantyState::Ended => format!("Ended {}", text::date(end)),
            _ => format!("Until {}", text::date(end)),
        };
        if let Some(p) = &wr.provider {
            w.push_str(&format!(", {p}"));
        }
        if let Some(r) = &wr.reference {
            if sens(r.sensitivity) && !opts.include_sensitive {
                *withheld += 1;
            } else {
                w.push_str(&format!(", ref. {}", r.value));
            }
        }
    }
    let shown =
        |a: &&housewright_core::model::Attachment| opts.include_sensitive || !sens(a.sensitivity);
    let hidden = it
        .attachments
        .iter()
        .filter(|a| sens(a.sensitivity) && !opts.include_sensitive)
        .count();
    *withheld += hidden;
    let atts: Vec<_> = it.attachments.iter().filter(shown).collect();
    let is_photo = |a: &&&housewright_core::model::Attachment| {
        a.media_type.starts_with("image/") || a.thumb_sha256.is_some()
    };
    let photo = atts
        .iter()
        .filter(is_photo)
        .find(|a| a.kind == AttachmentKind::Photo)
        .or_else(|| atts.iter().find(is_photo))
        .map(|a| format!("img/{}.jpg", a.sha256));
    let receipts = atts
        .iter()
        .filter(|a| a.kind == AttachmentKind::Receipt)
        .map(|a| a.name.clone())
        .collect();
    Line {
        room: d
            .room_name(i.room)
            .map(str::to_string)
            .unwrap_or_else(|| "Not in a room".into()),
        name: i.name.clone(),
        make: i.make.clone().unwrap_or_default(),
        model: i.model.clone().unwrap_or_default(),
        serial,
        purchased: i.purchased.map(|x| x.to_string()).unwrap_or_default(),
        installed: i.installed.map(|x| x.to_string()).unwrap_or_default(),
        price: i.price.clone(),
        value: i.value.clone(),
        warranty: w,
        warranty_ends: ends,
        receipts,
        photos: atts.iter().filter(is_photo).count(),
        photo,
        sensitive: whole,
    }
}

/// Gather the inventory for one home.
pub fn inventory(d: &HouseData, opts: InsuranceOptions) -> Inventory {
    let mut lines = Vec::new();
    let mut withheld_items = 0;
    let mut withheld_details = 0;
    for it in &d.items {
        if sens(it.item.sensitivity) && !opts.include_sensitive {
            withheld_items += 1;
            continue;
        }
        lines.push(line(d, it, opts, &mut withheld_details));
    }
    lines.sort_by(|a, b| {
        (a.room == "Not in a room")
            .cmp(&(b.room == "Not in a room"))
            .then(a.room.to_lowercase().cmp(&b.room.to_lowercase()))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let (mut total_price, mut total_value, mut other) = (0, 0, 0);
    for l in &lines {
        for (m, total) in [(&l.price, &mut total_price), (&l.value, &mut total_value)] {
            match m {
                Some(m) if m.currency == d.currency => *total += m.minor,
                Some(_) => other += 1,
                None => {}
            }
        }
    }
    Inventory {
        lines,
        withheld_items,
        withheld_details,
        total_price,
        total_value,
        other_currency: other,
        currency: d.currency.clone(),
    }
}

/// Whether anything worth an insurer's time is recorded about a thing.
fn has_details(l: &Line) -> bool {
    !(l.make.is_empty()
        && l.model.is_empty()
        && l.serial.is_empty()
        && l.purchased.is_empty()
        && l.price.is_none()
        && l.value.is_none()
        && l.warranty.is_empty()
        && l.receipts.is_empty()
        && l.photo.is_none())
}

fn money(m: &Option<Money>) -> String {
    m.as_ref()
        .map(|m| text::money(m.minor, &m.currency))
        .unwrap_or_default()
}

/// The inventory as a printable document: a cover, one section per room,
/// a list of receipts, and the totals.
pub fn document(d: &HouseData, inv: &Inventory, opts: InsuranceOptions) -> Document {
    let mut sections = Vec::new();
    let mut cover = Section::new("cover", "Cover", "For your insurer");
    let mut lines = vec![format!("Prepared {}", text::date(d.today))];
    if let Some(a) = &d.property.address
        && (!sens(a.sensitivity) || opts.include_sensitive)
    {
        lines.insert(0, a.value.clone());
    }
    lines.push(format!(
        "{} {} recorded",
        inv.lines.len(),
        if inv.lines.len() == 1 {
            "thing"
        } else {
            "things"
        }
    ));
    if inv.total_value > 0 {
        lines.push(format!(
            "Estimated value: {}",
            text::money(inv.total_value, &inv.currency)
        ));
    }
    if inv.total_price > 0 {
        lines.push(format!(
            "Paid when bought: {}",
            text::money(inv.total_price, &inv.currency)
        ));
    }
    cover.push(Block::Cover {
        title: "Home inventory".into(),
        subtitle: d.property.name.clone(),
        lines,
    });
    cover.push(Block::Para {
        text: "A record of the things in this home for an insurer: what they are, when they were bought, what they cost and what they are worth now. Values are the household's own estimates, not a valuation.".into(),
    });
    if opts.include_sensitive {
        cover.push(Block::Note {
            text: "This copy includes things and details marked sensitive, such as serial numbers of valuables. Keep it somewhere safe and send it only to your insurer.".into(),
        });
    } else if inv.withheld_items + inv.withheld_details > 0 {
        cover.push(Block::Note {
            text: format!(
                "Left out because they are marked sensitive: {} {} and {} {}. Make a copy with sensitive items included if your insurer needs them.",
                inv.withheld_items,
                if inv.withheld_items == 1 { "thing" } else { "things" },
                inv.withheld_details,
                if inv.withheld_details == 1 { "detail" } else { "details" },
            ),
        });
    }
    if inv.other_currency > 0 {
        cover.push(Block::Note {
            text: format!(
                "{} amounts in another currency are listed but not added to the totals.",
                inv.other_currency
            ),
        });
    }
    sections.push(cover);

    let mut by_room: BTreeMap<(bool, String), Vec<&Line>> = BTreeMap::new();
    for l in &inv.lines {
        by_room
            .entry((l.room == "Not in a room", l.room.clone()))
            .or_default()
            .push(l);
    }
    let mut summary = Vec::new();
    // Rooms follow one another (an inventory is printed whole, not
    // reprinted by section); things with nothing recorded about them yet
    // (a roof, the gutters) are named in one line instead of a box each.
    let mut s = Section::new("things", "Inventory", "Room by room");
    for ((_, room), ls) in &by_room {
        s.push(Block::Heading { text: room.clone() });
        let (detailed, bare): (Vec<&&Line>, Vec<&&Line>) = ls.iter().partition(|l| has_details(l));
        let entries: Vec<InventoryEntry> = detailed
            .iter()
            .map(|l| {
                let mut rows = Vec::new();
                let mm = [l.make.as_str(), l.model.as_str()]
                    .into_iter()
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !mm.is_empty() {
                    rows.push(("Make and model".to_string(), mm));
                }
                if !l.serial.is_empty() {
                    rows.push(("Serial".into(), l.serial.clone()));
                }
                if !l.purchased.is_empty() {
                    rows.push((
                        "Bought".into(),
                        l.purchased
                            .parse()
                            .map(text::date)
                            .unwrap_or_else(|_| l.purchased.clone()),
                    ));
                }
                if l.price.is_some() {
                    rows.push(("Price paid".into(), money(&l.price)));
                }
                if l.value.is_some() {
                    rows.push(("Estimated value".into(), money(&l.value)));
                }
                if !l.warranty.is_empty() {
                    rows.push(("Warranty".into(), l.warranty.clone()));
                }
                if !l.receipts.is_empty() {
                    rows.push(("Receipts".into(), l.receipts.join(", ")));
                }
                InventoryEntry {
                    name: l.name.clone(),
                    photo: l.photo.clone(),
                    rows,
                }
            })
            .collect();
        if !entries.is_empty() {
            s.push(Block::Inventory { entries });
        }
        if !bare.is_empty() {
            s.push(Block::Note {
                text: format!(
                    "Also here, with no details recorded yet: {}.",
                    bare.iter()
                        .map(|l| l.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
        let sum = |f: fn(&Line) -> &Option<Money>| -> i64 {
            ls.iter()
                .filter_map(|l| f(l).as_ref())
                .filter(|m| m.currency == inv.currency)
                .map(|m| m.minor)
                .sum()
        };
        summary.push(vec![
            room.clone(),
            ls.len().to_string(),
            text::money(sum(|l| &l.price), &inv.currency),
            text::money(sum(|l| &l.value), &inv.currency),
        ]);
    }
    sections.push(s);

    let mut totals = Section::new("totals", "Totals", "Totals and receipts");
    summary.push(vec![
        "All rooms".into(),
        inv.lines.len().to_string(),
        text::money(inv.total_price, &inv.currency),
        text::money(inv.total_value, &inv.currency),
    ]);
    totals.push(Block::Table {
        columns: vec![
            "Room".into(),
            "Things".into(),
            "Price paid".into(),
            "Estimated value".into(),
        ],
        rows: summary,
    });
    totals.push(Block::Heading {
        text: "Receipts".into(),
    });
    let rows: Vec<Vec<String>> = inv
        .lines
        .iter()
        .flat_map(|l| {
            l.receipts
                .iter()
                .map(|r| vec![l.name.clone(), l.room.clone(), r.clone()])
        })
        .collect();
    if rows.is_empty() {
        totals.push(Block::Note {
            text: "No receipts kept in the app yet. Add a photo or PDF of a receipt on a thing's page, with the kind \"Receipt\".".into(),
        });
    } else {
        totals.push(Block::Note {
            text: "The receipt files themselves are kept in the app and in its full backup.".into(),
        });
        totals.push(Block::Table {
            columns: vec!["Thing".into(), "Room".into(), "Receipt".into()],
            rows,
        });
    }

    sections.push(totals);

    Document {
        edition: Edition::Insurance,
        volume: Volume::Main,
        title: format!("Home inventory: {}", d.property.name),
        sections,
    }
}

/// One spreadsheet cell: quoted when needed, and never read as a formula
/// by a spreadsheet program (a leading `=`, `+`, `-`, `@`, tab or carriage
/// return gets an apostrophe).
fn cell(s: &str) -> String {
    let mut s = s.replace('\r', " ");
    if s.starts_with(['=', '+', '-', '@', '\t']) {
        s.insert(0, '\'');
    }
    if s.contains([',', '"', '\n']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn decimal(m: &Option<Money>) -> String {
    m.as_ref()
        .map(|m| {
            let neg = if m.minor < 0 { "-" } else { "" };
            format!(
                "{neg}{}.{:02}",
                m.minor.unsigned_abs() / 100,
                m.minor.unsigned_abs() % 100
            )
        })
        .unwrap_or_default()
}

/// The inventory as CSV (RFC 4180, UTF-8 with a byte order mark so
/// spreadsheet programs read accents correctly, CRLF line ends).
pub fn csv(inv: &Inventory) -> String {
    let mut out = String::from("\u{feff}");
    let header = [
        "Room",
        "Thing",
        "Make",
        "Model",
        "Serial",
        "Bought",
        "Installed",
        "Price paid",
        "Estimated value",
        "Currency",
        "Warranty",
        "Warranty ends",
        "Receipts",
        "Photos",
    ];
    out.push_str(&header.join(","));
    out.push_str("\r\n");
    for l in &inv.lines {
        let currency = l
            .value
            .as_ref()
            .or(l.price.as_ref())
            .map(|m| m.currency.clone())
            .unwrap_or_default();
        let row = [
            l.room.clone(),
            l.name.clone(),
            l.make.clone(),
            l.model.clone(),
            l.serial.clone(),
            l.purchased.clone(),
            l.installed.clone(),
            decimal(&l.price),
            decimal(&l.value),
            currency,
            l.warranty.clone(),
            l.warranty_ends.clone(),
            l.receipts.join("; "),
            l.photos.to_string(),
        ];
        out.push_str(&row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out.push_str(&format!(
        "Total,,,,,,,{},{},{},,,,\r\n",
        decimal(&Some(Money {
            minor: inv.total_price,
            currency: inv.currency.clone()
        })),
        decimal(&Some(Money {
            minor: inv.total_value,
            currency: inv.currency.clone()
        })),
        inv.currency
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_quoted_and_never_formulas() {
        assert_eq!(cell("plain"), "plain");
        assert_eq!(cell("a,b"), "\"a,b\"");
        assert_eq!(cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(cell("+1"), "'+1");
        assert_eq!(cell("@SUM"), "'@SUM");
        assert_eq!(cell("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn decimals() {
        let m = |minor| {
            Some(Money {
                minor,
                currency: "USD".into(),
            })
        };
        assert_eq!(decimal(&m(129999)), "1299.99");
        assert_eq!(decimal(&m(5)), "0.05");
        assert_eq!(decimal(&None), "");
    }
}
