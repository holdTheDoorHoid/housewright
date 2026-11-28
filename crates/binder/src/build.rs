//! The editions (D-42), as documents.
//!
//! Main-volume sections are built from a [`Public`] only, so they cannot
//! see a sensitive value. The sealed volume is built from the full data
//! and prints only what is sensitive.

use housewright_core::model::{AttachmentKind, EntityKind, Sensitivity};

use crate::data::{FactData, HouseData, ItemData, SEALED_LOCATION_KEY, TaskData};
use crate::doc::{Action, Block, Document, Edition, Photo, Section, Volume};
use crate::redact::Public;
use crate::text;

/// Choices that change what is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Options {
    /// Print what work cost (the handover edition may leave it out).
    pub include_costs: bool,
    /// Print "quirks only you know" notes (the handover edition may leave
    /// them out).
    pub include_notes: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            include_costs: true,
            include_notes: true,
        }
    }
}

impl Options {
    pub fn for_edition(e: Edition) -> Self {
        match e {
            Edition::Handover => Self {
                include_costs: false,
                include_notes: true,
            },
            _ => Self::default(),
        }
    }
}

/// Section names in the sealed volume, referred to from the main volume.
pub const SEALED_CODES: &str = "Codes and access";
pub const SEALED_ACCOUNTS: &str = "Accounts and utilities";
pub const SEALED_TRANSFER: &str = "Accounts to transfer";
pub const SEALED_PRIVATE: &str = "Valuables and private details";

const PREVIOUS_OWNER: &str = "Previous owner";
/// At most this many rows of history are printed.
const HISTORY_ROWS: usize = 60;

fn photo_of(a: &housewright_core::model::Attachment, caption: &str) -> Option<Photo> {
    let is_photo = a.media_type.starts_with("image/") || a.thumb_sha256.is_some();
    is_photo.then(|| Photo {
        file: format!("img/{}.jpg", a.sha256),
        caption: caption.to_string(),
    })
}

fn kind_label(k: AttachmentKind) -> &'static str {
    match k {
        AttachmentKind::Manual => "Manual",
        AttachmentKind::Receipt => "Receipt",
        AttachmentKind::Warranty => "Warranty",
        AttachmentKind::Photo => "Photo",
        AttachmentKind::Other => "Document",
    }
}

struct Ctx<'a> {
    d: &'a HouseData,
    edition: Edition,
    opts: Options,
}

impl Ctx<'_> {
    fn handover(&self) -> bool {
        self.edition == Edition::Handover
    }

    fn who(&self, m: housewright_core::ids::MemberId) -> String {
        if self.handover() {
            PREVIOUS_OWNER.into()
        } else {
            self.d.member_name(m).to_string()
        }
    }

    fn notes(&self, kind: EntityKind, id: &str) -> Option<Block> {
        if !self.opts.include_notes {
            return None;
        }
        let items: Vec<String> = self
            .d
            .notes_about(kind, id)
            .iter()
            .map(|n| n.text.value.clone())
            .collect();
        (!items.is_empty()).then_some(Block::Bullets { items })
    }

    fn fact(&self, key: &str) -> Option<&FactData> {
        self.d
            .facts
            .iter()
            .find(|f| f.fact.key.as_deref() == Some(key))
    }
}

// ---------------------------------------------------------------------
// The emergency page
// ---------------------------------------------------------------------

/// Shutoff facts, most important first: the suggested questions, then any
/// other fact in the shutoffs group, then anything about breakers.
fn shutoff_facts<'a>(c: &'a Ctx<'_>) -> Vec<&'a FactData> {
    let mut v: Vec<&FactData> = Vec::new();
    for k in [
        "water_main",
        "gas_shutoff",
        "electrical_panel",
        "well_pressure_switch",
    ] {
        if let Some(f) = c.fact(k) {
            v.push(f);
        }
    }
    for f in &c.d.facts {
        let breaker = f.fact.label.to_lowercase().contains("breaker");
        if (f.fact.category == "shutoffs" || breaker)
            && !v.iter().any(|x| x.fact.id == f.fact.id)
            && f.fact.key.as_deref() != Some(SEALED_LOCATION_KEY)
        {
            v.push(f);
        }
    }
    v.truncate(7);
    v
}

fn fact_place(c: &Ctx<'_>, f: &FactData) -> String {
    match c.d.room_name(f.fact.room) {
        Some(r) if !f.fact.value.contains(r) => format!("{} ({r})", f.fact.value),
        _ => f.fact.value.clone(),
    }
}

fn emergency_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("emergency", "Emergency", "In an emergency");
    let shut = shutoff_facts(c);
    let shutoffs: Vec<(String, String)> = shut
        .iter()
        .map(|f| (clip(&f.fact.label, 60), clip(&fact_place(c, f), 140)))
        .collect();
    let photos: Vec<Photo> = shut
        .iter()
        .flat_map(|f| {
            f.attachments
                .iter()
                .filter_map(|a| photo_of(a, &f.fact.label))
                .take(1)
        })
        .take(3)
        .collect();

    let mut calls: Vec<(String, String)> = Vec::new();
    for k in c.d.contractors.iter().filter(|k| k.emergency) {
        let how = [k.phone.clone(), k.email.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let title = if k.trades.is_empty() {
            k.name.clone()
        } else {
            format!("{}: {}", capitalise(&k.trades.join(", ")), k.name)
        };
        let how = if how.is_empty() {
            "No number recorded".into()
        } else {
            how
        };
        calls.push((clip(&title, 60), clip(&how, 80)));
    }
    for f in c.d.facts.iter().filter(|f| f.fact.category == "utilities") {
        calls.push((clip(&f.fact.label, 60), clip(&f.fact.value, 80)));
    }
    calls.truncate(6);

    let loc = |k: &str| c.fact(k).map(|f| fact_place(c, f));
    let water = match loc("water_main") {
        Some(l) => format!("Turn off the water main: {l}."),
        None => "Turn off the water main. (Its place is not recorded: find it before you need it.)"
            .into(),
    };
    let panel = match loc("electrical_panel") {
        Some(l) => format!(" The panel is here: {l}."),
        None => String::new(),
    };
    let plumber =
        c.d.contractors
            .iter()
            .find(|k| k.emergency && k.trades.iter().any(|t| t.to_lowercase().contains("plumb")))
            .map(|k| match &k.phone {
                Some(p) => format!("Call a plumber: {} {p}.", k.name),
                None => format!("Call a plumber: {}.", k.name),
            })
            .unwrap_or_else(|| "Call a plumber.".into());
    let actions = vec![
        Action {
            title: "Gas smell".into(),
            steps: vec![
                "Do not switch lights or anything electrical on or off. No flames.".into(),
                "Get everyone out now. Leave the door open behind you.".into(),
                "From outside, call 911 or the gas company's emergency line.".into(),
                "Stay out until they say it is safe.".into(),
            ],
        },
        Action {
            title: "Carbon monoxide alarm".into(),
            steps: vec![
                "Get everyone, and pets, outside into fresh air now.".into(),
                "Call 911 from outside. Say if anyone feels dizzy, sick or has a headache.".into(),
                "Stay out until responders say it is safe.".into(),
            ],
        },
        Action {
            title: "Water leak".into(),
            steps: vec![
                water,
                "Keep away from water near outlets or appliances. Switch off power only if you can stand somewhere dry.".into(),
                plumber,
            ],
        },
        Action {
            title: "No power".into(),
            steps: vec![
                "Are the neighbours dark too? Then report the outage to the electric company.".into(),
                format!(
                    "Only your home? Check the panel for a switch that is off or in the middle: push it fully off, then on.{panel}"
                ),
                "Use flashlights, not candles. Never run a generator indoors or in a garage.".into(),
            ],
        },
    ];
    s.push(Block::Emergency {
        shutoffs,
        photos,
        calls,
        actions,
    });
    s
}

/// The emergency page must stay one page: very long entries are cut,
/// with an ellipsis, and the rest of the text is in the binder.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------
// Main volume sections
// ---------------------------------------------------------------------

fn about_section(c: &Ctx<'_>, contents: &[(String, String)]) -> Section {
    let mut s = Section::new("about", "About", "About this binder");
    let name = &c.d.property.name;
    let intro = match c.edition {
        Edition::Handover => format!(
            "This binder is for the next owner or tenant of {name}. It describes the house's systems, how old they are, how they have been looked after and what is due next. Personal details have been left out."
        ),
        Edition::Succession => format!(
            "This binder is for whoever looks after {name} next. It holds what the people who lived here knew: where things are, how they are looked after, who to call, and what is due. Accounts and codes are in a separate sealed volume."
        ),
        _ => format!(
            "This binder holds what the household knows about running {name}: where things are, how they are looked after, who to call and what is due. It is meant to make sense on its own, without the app."
        ),
    };
    s.push(Block::Para { text: intro });
    s.push(Block::Bullets {
        items: vec![
            "Each section starts on a new page and has its own page numbers (\"Water 3\"), so one section can be reprinted and swapped in on its own.".into(),
            "Every page says the date it was printed. If something looks out of date, the app knows more.".into(),
            "Codes printed on stickers (HW-…) match the stickers on the things themselves.".into(),
        ],
    });
    if c.edition.has_sealed() {
        s.push(Block::Heading {
            text: "The sealed volume".into(),
        });
        s.push(Block::Para {
            text: "Codes, passwords, account numbers and where valuables are kept are not in this binder. They are in a separate sealed volume.".into(),
        });
        match c.d.sealed_location() {
            Some(l) => s.push(Block::Pairs {
                rows: vec![("Where it is kept".into(), l.to_string())],
            }),
            None => s.push(Block::WriteIn {
                label: "The sealed volume is kept:".into(),
            }),
        }
    }
    s.push(Block::Heading {
        text: "What is in this binder".into(),
    });
    s.push(Block::Table {
        columns: vec!["Section".into(), "Covers".into()],
        rows: contents
            .iter()
            .map(|(a, b)| vec![a.clone(), b.clone()])
            .collect(),
    });
    s
}

fn house_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("house", "House", "The house");
    let mut rows = vec![("Name".to_string(), c.d.property.name.clone())];
    if let Some(a) = &c.d.property.address {
        rows.push(("Address".into(), a.value.clone()));
    }
    if let Some(p) = &c.d.profile {
        rows.extend(text::profile(p));
    }
    if !c.d.rooms.is_empty() {
        let mut names: Vec<&str> = c.d.rooms.values().map(|s| s.as_str()).collect();
        names.sort();
        rows.push(("Rooms recorded".into(), names.join(", ")));
    }
    let n = c.d.items.len();
    rows.push((
        "Things recorded".into(),
        format!("{n} {}", if n == 1 { "thing" } else { "things" }),
    ));
    s.push(Block::Pairs { rows });
    if let Some(b) = c.notes(EntityKind::Property, &c.d.property.id.to_string()) {
        s.push(Block::Heading {
            text: "Quirks only you know".into(),
        });
        s.push(b);
    }
    if c.opts.include_notes {
        let mut any = false;
        let mut rooms: Vec<(&housewright_core::ids::RoomId, &String)> = c.d.rooms.iter().collect();
        rooms.sort_by_key(|(_, n)| n.to_lowercase());
        for (id, name) in rooms {
            if let Some(b) = c.notes(EntityKind::Room, &id.to_string()) {
                if !any {
                    s.push(Block::Heading {
                        text: "Room by room".into(),
                    });
                    any = true;
                }
                s.push(Block::Subheading { text: name.clone() });
                s.push(b);
            }
        }
    }
    s
}

fn facts_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("facts", "Facts", "Home facts");
    s.push(Block::Note {
        text: "Quick answers: shutoffs, sizes, paint colours and the like.".into(),
    });
    let mut any = false;
    for (key, label, _) in housewright_core::facts::GROUPS {
        let rows: Vec<(String, String)> =
            c.d.facts
                .iter()
                .filter(|f| {
                    f.fact.category == *key && f.fact.key.as_deref() != Some(SEALED_LOCATION_KEY)
                })
                .map(|f| (f.fact.label.clone(), fact_place(c, f)))
                .collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        s.push(Block::Heading {
            text: (*label).into(),
        });
        s.push(Block::Pairs { rows });
        let photos: Vec<Photo> =
            c.d.facts
                .iter()
                .filter(|f| f.fact.category == *key)
                .flat_map(|f| {
                    f.attachments
                        .iter()
                        .filter_map(|a| photo_of(a, &f.fact.label))
                })
                .take(6)
                .collect();
        if !photos.is_empty() {
            s.push(Block::Photos { photos });
        }
    }
    if !any {
        s.push(Block::Para {
            text: "No facts recorded yet.".into(),
        });
    }
    let hidden: usize = c.d.withheld.facts.values().sum();
    if hidden > 0 && c.edition.has_sealed() {
        s.push(Block::Note {
            text: format!(
                "{hidden} more {} kept in the sealed volume (sections \"{SEALED_CODES}\" and \"{}\").",
                if hidden == 1 { "fact is" } else { "facts are" },
                if c.edition == Edition::Succession {
                    SEALED_TRANSFER
                } else {
                    SEALED_ACCOUNTS
                }
            ),
        });
    }
    s
}

fn last_serviced(it: &ItemData, c: &Ctx<'_>) -> Option<jiff::civil::Date> {
    let id = it.item.id;
    c.d.completions
        .iter()
        .filter(|x| x.item == Some(id))
        .map(|x| x.at)
        .max()
}

fn next_due(tasks: &[TaskData]) -> Option<(jiff::civil::Date, String)> {
    tasks
        .iter()
        .filter_map(|t| t.eval.window.map(|w| (w.due, t.task.title.clone())))
        .min_by_key(|(d, _)| *d)
}

fn task_block(c: &Ctx<'_>, t: &TaskData, s: &mut Section) {
    s.push(Block::Subheading {
        text: t.task.title.clone(),
    });
    let mut rows = vec![("How often".to_string(), text::schedule(&t.task.schedule))];
    rows.push((
        "Last done".into(),
        t.eval
            .last_done
            .map(text::date)
            .unwrap_or_else(|| "Not recorded".into()),
    ));
    if let Some(w) = t.eval.window {
        let due = if w.due < c.d.today {
            format!("{} (overdue)", text::date(w.due))
        } else {
            text::date(w.due)
        };
        rows.push(("Next due".into(), due));
    }
    if let Some(m) = t.task.minutes {
        rows.push(("Takes about".into(), format!("{m} minutes")));
    }
    if !c.handover() {
        rows.push(("Looked after by".into(), c.who(t.task.owner)));
    }
    if let Some(w) = &t.task.why {
        rows.push(("Why".into(), format!("{} (Source: {})", w.text, w.source)));
    }
    s.push(Block::Pairs { rows });
    if !t.task.steps.is_empty() {
        s.push(Block::Steps {
            items: t.task.steps.clone(),
        });
    }
}

fn item_block(c: &Ctx<'_>, it: &ItemData, s: &mut Section) {
    let i = &it.item;
    s.push(Block::Heading {
        text: i.name.clone(),
    });
    let mut rows: Vec<(String, String)> = Vec::new();
    let place = [
        c.d.room_name(i.room).map(|s| s.to_string()),
        i.location_note.as_ref().map(|l| l.value.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(": ");
    if !place.is_empty() {
        rows.push(("Where".into(), place));
    }
    if let Some(m) = &i.make {
        rows.push(("Make".into(), m.clone()));
    }
    if let Some(m) = &i.model {
        rows.push(("Model".into(), m.clone()));
    }
    if let Some(sn) = &i.serial {
        rows.push(("Serial".into(), sn.value.clone()));
    }
    if let Some(d) = i.installed {
        let age = text::age(d, c.d.today).unwrap_or_default();
        rows.push(("Installed".into(), format!("{} ({age})", text::date(d))));
    }
    if let Some(d) = i.purchased {
        rows.push(("Bought".into(), text::date(d)));
    }
    if let Some(w) = &i.warranty {
        let mut v = format!(
            "{} {}",
            if w.expires < c.d.today {
                "Ended"
            } else {
                "Until"
            },
            text::date(w.expires)
        );
        if let Some(p) = &w.provider {
            v.push_str(&format!(", {p}"));
        }
        if let Some(r) = &w.reference {
            v.push_str(&format!(", ref. {}", r.value));
        }
        v.push_str(if w.transferable {
            ". Transfers to a new owner."
        } else if c.handover() {
            ". Does not transfer to a new owner."
        } else {
            "."
        });
        rows.push(("Warranty".into(), v));
    }
    if let Some(d) = last_serviced(it, c) {
        rows.push(("Last serviced".into(), text::date(d)));
    }
    if let Some((d, what)) = next_due(&it.tasks) {
        rows.push(("Next due".into(), format!("{}: {what}", text::date(d))));
    }
    for f in &it.fields {
        if let Some(v) = i
            .custom
            .get(&f.key)
            .and_then(|v| text::field_value(&v.value))
        {
            rows.push((f.label.clone(), v));
        }
    }
    rows.push((
        "Sticker code".into(),
        housewright_core::labels::short_code(i.id),
    ));
    s.push(Block::Pairs { rows });
    let photos: Vec<Photo> = it
        .attachments
        .iter()
        .filter(|a| a.kind == AttachmentKind::Photo)
        .filter_map(|a| photo_of(a, &i.name))
        .take(1)
        .collect();
    if !photos.is_empty() {
        s.push(Block::Photos { photos });
    }
    if let Some(b) = c.notes(EntityKind::Item, &i.id.to_string()) {
        s.push(Block::Subheading {
            text: "Quirks only you know".into(),
        });
        s.push(b);
    }
    let docs: Vec<String> = it
        .attachments
        .iter()
        .filter(|a| a.kind != AttachmentKind::Photo)
        .map(|a| format!("{}: {}", kind_label(a.kind), a.name))
        .collect();
    if !docs.is_empty() {
        s.push(Block::Subheading {
            text: "Documents in the app".into(),
        });
        s.push(Block::Bullets { items: docs });
    }
    for t in &it.tasks {
        task_block(c, t, s);
    }
    if it.tasks.is_empty() {
        s.push(Block::Note {
            text: "No regular maintenance switched on for this.".into(),
        });
    }
}

fn things_sections(c: &Ctx<'_>) -> Vec<Section> {
    let mut out = Vec::new();
    for cat in text::CATEGORY_ORDER {
        let mut its: Vec<&ItemData> =
            c.d.items
                .iter()
                .filter(|i| {
                    let k = i.item.category.as_deref().unwrap_or("general");
                    k == cat || (cat == "general" && !text::CATEGORY_ORDER.contains(&k))
                })
                .collect();
        let house_tasks = cat == "general" && !c.d.house_tasks.is_empty();
        if its.is_empty() && !house_tasks {
            continue;
        }
        its.sort_by_key(|i| {
            (
                c.d.room_name(i.item.room).unwrap_or("~").to_lowercase(),
                i.item.name.to_lowercase(),
            )
        });
        let (short, title) = text::category(Some(cat));
        let mut s = Section::new(format!("things:{cat}"), short, title);
        for it in its {
            item_block(c, it, &mut s);
        }
        if house_tasks {
            s.push(Block::Heading {
                text: "Around the house".into(),
            });
            for t in &c.d.house_tasks {
                task_block(c, t, &mut s);
            }
        }
        out.push(s);
    }
    out
}

fn calendar_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("calendar", "Calendar", "The next 12 months");
    s.push(Block::Note {
        text: "What falls due, month by month, if each job is done when it is due.".into(),
    });
    // Every occurrence, per job.
    let mut jobs: Vec<(String, Vec<jiff::civil::Date>)> = Vec::new();
    let all =
        c.d.items
            .iter()
            .flat_map(|i| i.tasks.iter().map(move |t| (Some(i.item.name.as_str()), t)))
            .chain(c.d.house_tasks.iter().map(|t| (None, t)));
    for (thing, t) in all {
        if t.upcoming.is_empty() {
            continue;
        }
        let label = match thing {
            Some(n) if !t.task.title.contains(n) => format!("{} ({n})", t.task.title),
            _ => t.task.title.clone(),
        };
        jobs.push((label, t.upcoming.clone()));
    }
    if jobs.is_empty() {
        s.push(Block::Para {
            text: "Nothing is scheduled. Switch on some tasks in the app to fill this page.".into(),
        });
        return s;
    }
    jobs.sort();
    // Monthly (or more often) jobs would fill every row: list them once.
    let (often, rest): (Vec<_>, Vec<_>) = jobs.into_iter().partition(|(_, d)| d.len() >= 10);
    if !often.is_empty() {
        s.push(Block::Subheading {
            text: "Every month".into(),
        });
        s.push(Block::Bullets {
            items: often.iter().map(|(l, _)| l.clone()).collect(),
        });
    }
    let today = c.d.today;
    let mut months: std::collections::BTreeMap<(i16, i8), Vec<(jiff::civil::Date, String)>> =
        std::collections::BTreeMap::new();
    let mut overdue: Vec<String> = Vec::new();
    for (label, dates) in &rest {
        for d in dates {
            if *d < today {
                overdue.push(format!("{label}, since {}", text::date(*d)));
            } else {
                months
                    .entry((d.year(), d.month()))
                    .or_default()
                    .push((*d, label.clone()));
            }
        }
    }
    let mut groups = Vec::new();
    if !overdue.is_empty() {
        groups.push(("Already due".to_string(), overdue));
    }
    for ((y, m), mut v) in months {
        v.sort();
        let name = jiff::civil::Date::new(y, m, 1)
            .map(text::month)
            .unwrap_or_default();
        let lines: Vec<String> = v
            .into_iter()
            .map(|(d, l)| format!("{}  {l}", d.day()))
            .collect();
        groups.push((name, lines));
    }
    s.push(Block::Subheading {
        text: "Month by month".into(),
    });
    s.push(Block::Agenda { groups });
    s
}

fn history_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("history", "History", "Maintenance history");
    if c.d.completions.is_empty() {
        s.push(Block::Para {
            text: "Nothing recorded yet.".into(),
        });
        return s;
    }
    let names: std::collections::BTreeMap<String, String> =
        c.d.items
            .iter()
            .map(|i| (i.item.id.to_string(), i.item.name.clone()))
            .collect();
    let mut cols = vec![
        "Date".into(),
        "Job".into(),
        "Thing".into(),
        "Done by".into(),
    ];
    if c.opts.include_costs {
        cols.push("Cost".into());
    }
    let mut rows = Vec::new();
    let mut total = 0i64;
    for x in c.d.completions.iter().take(HISTORY_ROWS) {
        let thing = x
            .item
            .and_then(|i| names.get(&i.to_string()).cloned())
            .unwrap_or_else(|| "The house".into());
        let by = match x
            .contractor
            .and_then(|k| c.d.contractors.iter().find(|y| y.id == k))
        {
            Some(k) if c.handover() => k.name.clone(),
            Some(k) => format!("{} ({})", c.who(x.by), k.name),
            None => c.who(x.by),
        };
        let mut row = vec![text::date(x.at), x.task_title.clone(), thing, by];
        if c.opts.include_costs {
            let sum: i64 = x
                .costs
                .iter()
                .filter(|l| l.amount.currency == c.d.currency)
                .map(|l| l.amount.minor)
                .sum();
            total += sum;
            row.push(if sum == 0 {
                String::new()
            } else {
                text::money(sum, &c.d.currency)
            });
        }
        rows.push(row);
    }
    s.push(Block::Table {
        columns: cols,
        rows,
    });
    if c.d.completions.len() > HISTORY_ROWS {
        s.push(Block::Note {
            text: format!(
                "The {HISTORY_ROWS} most recent jobs are shown. {} older ones are in the app.",
                c.d.completions.len() - HISTORY_ROWS
            ),
        });
    }
    if c.opts.include_costs && total > 0 {
        s.push(Block::Note {
            text: format!(
                "Total of the costs above: {}.",
                text::money(total, &c.d.currency)
            ),
        });
    }
    s
}

fn documents_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("documents", "Documents", "Manuals and documents");
    let mut rows = Vec::new();
    for it in &c.d.items {
        for a in it
            .attachments
            .iter()
            .filter(|a| a.kind != AttachmentKind::Photo)
        {
            rows.push(vec![
                it.item.name.clone(),
                kind_label(a.kind).to_string(),
                a.name.clone(),
            ]);
        }
    }
    if rows.is_empty() {
        s.push(Block::Para {
            text: "No manuals or documents are stored in the app yet.".into(),
        });
    } else {
        s.push(Block::Note {
            text: "These files are stored in the app and in its full backup. Printed copies can be filed behind this page.".into(),
        });
        s.push(Block::Table {
            columns: vec!["Thing".into(), "Kind".into(), "File".into()],
            rows,
        });
    }
    s
}

fn contacts_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("contacts", "Contacts", "Who to call");
    if c.d.contractors.is_empty() {
        s.push(Block::Para {
            text: "No contractors recorded yet.".into(),
        });
        s.push(Block::WriteIn {
            label: "Plumber:".into(),
        });
        s.push(Block::WriteIn {
            label: "Electrician:".into(),
        });
        s.push(Block::WriteIn {
            label: "Heating and cooling:".into(),
        });
        return s;
    }
    for k in &c.d.contractors {
        s.push(Block::Subheading {
            text: if k.emergency {
                format!("{} (for emergencies)", k.name)
            } else {
                k.name.clone()
            },
        });
        let mut rows = Vec::new();
        if !k.trades.is_empty() {
            rows.push(("Does".to_string(), capitalise(&k.trades.join(", "))));
        }
        if let Some(p) = &k.phone {
            rows.push(("Phone".into(), p.clone()));
        }
        if let Some(e) = &k.email {
            rows.push(("Email".into(), e.clone()));
        }
        for (label, cred) in [("Licence", &k.licence), ("Insurance", &k.insurance)] {
            if let Some(x) = cred {
                let mut v = x.number.value.clone();
                if let Some(d) = x.expires {
                    v.push_str(&format!(" (until {})", text::date(d)));
                }
                rows.push((label.into(), v));
            }
        }
        if !c.handover()
            && let Some(n) = &k.notes
        {
            rows.push(("Notes".into(), n.value.clone()));
        }
        let jobs =
            c.d.completions
                .iter()
                .filter(|x| x.contractor == Some(k.id))
                .count();
        if jobs > 0 {
            rows.push((
                "Jobs here".into(),
                format!("{jobs} recorded (see Maintenance history)"),
            ));
        }
        s.push(Block::Pairs { rows });
    }
    s
}

fn accounts_section(c: &Ctx<'_>) -> Section {
    let mut s = Section::new("accounts", "Accounts", "Accounts and utilities to transfer");
    s.push(Block::Para {
        text: "Utilities and accounts that need to move to a new name. Tick each one off as it is done.".into(),
    });
    let rows: Vec<Vec<String>> =
        c.d.facts
            .iter()
            .filter(|f| f.fact.category == "utilities")
            .map(|f| vec![f.fact.label.clone(), f.fact.value.clone()])
            .collect();
    if !rows.is_empty() {
        s.push(Block::Checklist {
            columns: vec!["Account".into(), "Details".into()],
            rows: rows.clone(),
        });
    }
    let hidden = c.d.withheld.facts.get("utilities").copied().unwrap_or(0);
    if hidden > 0 {
        s.push(Block::Para {
            text: format!(
                "{hidden} {} with account numbers {} in the sealed volume, section \"{SEALED_TRANSFER}\".",
                if hidden == 1 { "account" } else { "accounts" },
                if hidden == 1 { "is" } else { "are" },
            ),
        });
    } else if rows.is_empty() {
        s.push(Block::Para {
            text: "No utilities or accounts are recorded. Common ones: electricity, gas, water, sewer, rubbish, internet, home insurance, property tax.".into(),
        });
    }
    s
}

/// The main volume of an edition. Built only from public data.
pub fn main_volume(p: &Public, edition: Edition, opts: Options) -> Document {
    let d = p.data();
    let c = Ctx { d, edition, opts };
    let title = format!("{}: {}", edition.label(), d.property.name);
    if edition == Edition::Emergency {
        return Document {
            edition,
            volume: Volume::Main,
            title,
            sections: vec![emergency_section(&c)],
        };
    }
    let mut body = vec![house_section(&c), facts_section(&c)];
    body.extend(things_sections(&c));
    body.push(calendar_section(&c));
    body.push(history_section(&c));
    body.push(documents_section(&c));
    body.push(contacts_section(&c));
    if edition == Edition::Succession {
        body.push(accounts_section(&c));
    }
    let emergency = emergency_section(&c);
    let mut contents: Vec<(String, String)> = vec![
        (emergency.short.clone(), emergency.title.clone()),
        ("About".into(), "About this binder".into()),
    ];
    contents.extend(body.iter().map(|s| (s.short.clone(), s.title.clone())));
    let about = about_section(&c, &contents);
    let mut sections = vec![emergency, about];
    sections.extend(body);
    Document {
        edition,
        volume: Volume::Main,
        title,
        sections,
    }
}

// ---------------------------------------------------------------------
// The sealed volume
// ---------------------------------------------------------------------

fn sens(s: Sensitivity) -> bool {
    s == Sensitivity::Sensitive
}

/// The sealed volume of an edition: only sensitive material. `None` for
/// editions without one.
pub fn sealed_volume(d: &HouseData, edition: Edition, opts: Options) -> Option<Document> {
    if !edition.has_sealed() {
        return None;
    }
    let c = Ctx { d, edition, opts };
    let title = format!("Sealed volume: {}", d.property.name);
    let mut cover = Section::new("sealed:cover", "Sealed", "SEALED — keep in a safe place");
    cover.push(Block::Cover {
        title: d.property.name.clone(),
        subtitle: "The sealed volume of the house binder".into(),
        lines: vec![
            "This volume holds the details that are not printed in the household binder: codes, passwords, account numbers and where valuables are kept.".into(),
            "Keep it somewhere safe and private, such as a fire safe or with a lawyer, and tell someone you trust where it is. The household binder has a page saying where.".into(),
            "If something has happened and you are now looking after the house: start with the household binder. Come here when it sends you.".into(),
        ],
    });

    let mut recovery = Section::new("sealed:recovery", "Recovery", "Recovery key");
    recovery.push(Block::Para {
        text: "Recovery key: set up when you turn on sync.".into(),
    });
    recovery.push(Block::Note {
        text: "When sync is switched on, the app makes a recovery key: the only way to unlock the household's data if every device is lost. Reprint this page then, and the key will be printed here.".into(),
    });

    let fact_rows = |f: &dyn Fn(&FactData) -> bool| -> Vec<(String, String)> {
        d.facts
            .iter()
            .filter(|x| sens(x.fact.sensitivity) && f(x))
            .map(|x| (x.fact.label.clone(), fact_place(&c, x)))
            .collect()
    };

    let mut codes = Section::new("sealed:codes", "Codes", SEALED_CODES);
    let mut any = false;
    for (key, label, _) in housewright_core::facts::GROUPS {
        if *key == "utilities" {
            continue;
        }
        let rows = fact_rows(&|x| x.fact.category == *key);
        if rows.is_empty() {
            continue;
        }
        any = true;
        codes.push(Block::Heading {
            text: (*label).into(),
        });
        codes.push(Block::Pairs { rows });
    }
    if !any {
        codes.push(Block::Para {
            text: "No codes or passwords recorded.".into(),
        });
    }

    let succession = edition == Edition::Succession;
    let mut accounts = Section::new(
        "sealed:accounts",
        "Accounts",
        if succession {
            SEALED_TRANSFER
        } else {
            SEALED_ACCOUNTS
        },
    );
    let rows = fact_rows(&|x| x.fact.category == "utilities");
    if rows.is_empty() {
        accounts.push(Block::Para {
            text: "No account details recorded.".into(),
        });
    } else if succession {
        accounts.push(Block::Para {
            text: "Each of these needs to move to a new name, or be closed. Tick each one off as it is done.".into(),
        });
        accounts.push(Block::Checklist {
            columns: vec!["Account".into(), "Details".into()],
            rows: rows.into_iter().map(|(a, b)| vec![a, b]).collect(),
        });
    } else {
        accounts.push(Block::Pairs { rows });
    }

    let mut private = Section::new("sealed:private", "Private", SEALED_PRIVATE);
    let mut any = false;
    for it in &d.items {
        let i = &it.item;
        let mut rows: Vec<(String, String)> = Vec::new();
        let whole = sens(i.sensitivity);
        if whole {
            if let Some(r) = d.room_name(i.room) {
                rows.push(("Room".into(), r.to_string()));
            }
            for (k, v) in [("Make", &i.make), ("Model", &i.model)] {
                if let Some(v) = v {
                    rows.push((k.into(), v.clone()));
                }
            }
        }
        if let Some(l) = &i.location_note
            && (whole || sens(l.sensitivity))
        {
            rows.push(("Where exactly".into(), l.value.clone()));
        }
        if let Some(sn) = &i.serial
            && (whole || sens(sn.sensitivity))
        {
            rows.push(("Serial".into(), sn.value.clone()));
        }
        if let Some(w) = &i.warranty
            && let Some(r) = &w.reference
            && (whole || sens(r.sensitivity))
        {
            rows.push(("Warranty reference".into(), r.value.clone()));
        }
        for f in &it.fields {
            if let Some(v) = i.custom.get(&f.key)
                && (whole || sens(v.sensitivity) || sens(f.sensitivity))
                && let Some(t) = text::field_value(&v.value)
            {
                rows.push((f.label.clone(), t));
            }
        }
        let notes: Vec<String> = d
            .notes_about(EntityKind::Item, &i.id.to_string())
            .iter()
            .filter(|n| whole || sens(n.text.sensitivity))
            .map(|n| n.text.value.clone())
            .collect();
        if rows.is_empty() && notes.is_empty() {
            continue;
        }
        any = true;
        private.push(Block::Heading {
            text: i.name.clone(),
        });
        if !rows.is_empty() {
            private.push(Block::Pairs { rows });
        }
        if !notes.is_empty() {
            private.push(Block::Bullets { items: notes });
        }
    }
    let house_notes: Vec<String> = d
        .notes
        .iter()
        .filter(|n| n.about.kind != EntityKind::Item && sens(n.text.sensitivity))
        .map(|n| {
            let place = match n.about.kind {
                EntityKind::Room => d
                    .rooms
                    .iter()
                    .find(|(id, _)| id.to_string() == n.about.id)
                    .map(|(_, name)| format!("{name}: "))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            format!("{place}{}", n.text.value)
        })
        .collect();
    if !house_notes.is_empty() {
        any = true;
        private.push(Block::Heading {
            text: "Notes about the house".into(),
        });
        private.push(Block::Bullets { items: house_notes });
    }
    let mut krows = Vec::new();
    for k in &d.contractors {
        if let Some(n) = &k.notes
            && sens(n.sensitivity)
        {
            krows.push((format!("{}: notes", k.name), n.value.clone()));
        }
        for (label, cred) in [("licence", &k.licence), ("insurance", &k.insurance)] {
            if let Some(x) = cred
                && sens(x.number.sensitivity)
            {
                krows.push((format!("{}: {label}", k.name), x.number.value.clone()));
            }
        }
    }
    if !krows.is_empty() {
        any = true;
        private.push(Block::Heading {
            text: "Contractors".into(),
        });
        private.push(Block::Pairs { rows: krows });
    }
    if !any {
        private.push(Block::Para {
            text: "Nothing private recorded about things in the house.".into(),
        });
    }

    Some(Document {
        edition,
        volume: Volume::Sealed,
        title,
        sections: vec![cover, recovery, codes, accounts, private],
    })
}
