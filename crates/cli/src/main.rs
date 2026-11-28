//! `hw`: a small command-line front end over the Housewright core, for testing
//! and for power users. Licensed AGPL-3.0-only.
//!
//! Things are referred to by name (case-insensitive) or by the short id shown
//! in listings (`#` followed by the last 8 characters of the id).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use housewright_core::audit::audit_log;
use housewright_core::export;
use housewright_core::fields::{CustomValue, FieldType, FieldValue};
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::priority::{PriorityClass, START_SMALL_DEFAULT, welcome_back};
use housewright_core::schedule::{Climate, Hemisphere, Interval, Schedule, Season};
use housewright_core::store::{ManualClock, Store};
use housewright_core::template::Library;
use housewright_core::{Error, Result};
use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

#[derive(Parser)]
#[command(
    name = "hw",
    version,
    about = "Housewright home maintenance (command line)"
)]
struct Cli {
    /// Database file.
    #[arg(long, env = "HW_DB", default_value = "housewright.db", global = true)]
    db: PathBuf,
    /// Pretend it is this date (YYYY-MM-DD, noon local) or instant (RFC 3339).
    #[arg(long, env = "HW_NOW", global = true)]
    now: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a household in a new database.
    Init {
        #[arg(long)]
        household: String,
        /// Your name; you become the owner.
        #[arg(long)]
        owner: String,
        /// IANA time zone, e.g. America/New_York.
        #[arg(long, default_value = "America/New_York")]
        tz: String,
    },
    /// Houses.
    #[command(subcommand)]
    Property(PropertyCmd),
    /// Rooms.
    #[command(subcommand)]
    Room(RoomCmd),
    /// Things in the house.
    #[command(subcommand)]
    Item(ItemCmd),
    /// Maintenance tasks.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Switch on the most important suggested tasks (start small).
    Start {
        #[arg(long, default_value_t = START_SMALL_DEFAULT)]
        n: usize,
    },
    /// After time away: the three overdue tasks that matter most.
    WelcomeBack,
    /// Show who did what.
    Audit,
    /// Write the whole household to a JSON file.
    Export { file: PathBuf },
    /// Load a JSON export into a new, empty database.
    Import { file: PathBuf },
    /// Check a template directory.
    Templates {
        #[arg(default_value = "templates")]
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum PropertyCmd {
    Add {
        #[arg(long)]
        name: String,
        /// Defaults to the household's time zone.
        #[arg(long)]
        tz: Option<String>,
        #[arg(long, value_enum, default_value_t = Hemi::North)]
        hemisphere: Hemi,
        /// Average first autumn frost, day of year (1-365).
        #[arg(long)]
        first_frost_doy: Option<u16>,
        /// Average last spring frost, day of year (1-365).
        #[arg(long)]
        last_frost_doy: Option<u16>,
    },
    List,
}

#[derive(Clone, Copy, ValueEnum)]
enum Hemi {
    North,
    South,
}

#[derive(Subcommand)]
enum RoomCmd {
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        property: Option<String>,
    },
    List,
}

// Parsed once per run; the size difference does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum ItemCmd {
    /// Add an item; with --template, its suggested tasks are added too
    /// (switched off until `hw start` or `hw task on`).
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        property: Option<String>,
        #[arg(long)]
        room: Option<String>,
        #[arg(long)]
        template: Option<String>,
        #[arg(long, env = "HW_TEMPLATES", default_value = "templates")]
        templates: PathBuf,
        #[arg(long)]
        make: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        serial: Option<String>,
        #[arg(long)]
        installed: Option<Date>,
        /// Template field value, key=value (repeatable).
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    List,
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Add a task by hand (switched on).
    Add(TaskAdd),
    /// List tasks, most important first.
    List {
        /// Include suggested tasks that are not switched on.
        #[arg(long)]
        all: bool,
    },
    /// Mark a task done.
    Done {
        task: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reschedule to a date.
    Snooze {
        task: String,
        #[arg(long)]
        until: Date,
    },
    /// Switch a suggested task on.
    On { task: String },
    /// Show a task's steps and why it matters.
    Show { task: String },
}

#[derive(Args)]
struct TaskAdd {
    #[arg(long)]
    title: String,
    #[arg(long)]
    item: Option<String>,
    #[arg(long)]
    property: Option<String>,
    /// Owner (defaults to you).
    #[arg(long)]
    owner: Option<String>,
    #[arg(long, value_enum, default_value_t = Prio::Efficiency)]
    priority: Prio,
    /// Fixed: every N units from today ("3 months").
    #[arg(long, group = "sched")]
    every: Option<Interval>,
    /// Floating: N units after last done ("90 days").
    #[arg(long, group = "sched")]
    after: Option<Interval>,
    /// Seasonal window.
    #[arg(long, group = "sched", value_enum)]
    season: Option<SeasonArg>,
    /// One-off on a date.
    #[arg(long, group = "sched")]
    once: Option<Date>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Prio {
    LifeSafety,
    PropertyLoss,
    Efficiency,
    Cosmetic,
}

#[derive(Clone, Copy, ValueEnum)]
enum SeasonArg {
    Spring,
    Summer,
    Autumn,
    Winter,
    BeforeFirstFrost,
    AfterLastFrost,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hw: {e}");
            ExitCode::FAILURE
        }
    }
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

/// Parse `--now`. A bare date is noon in `tz`.
fn parse_now(s: &str, tz: &TimeZone) -> Result<Timestamp> {
    if let Ok(t) = s.parse::<Timestamp>() {
        return Ok(t);
    }
    let d: Date = s
        .parse()
        .map_err(|e| invalid(format!("--now '{s}': {e}")))?;
    Ok(d.at(12, 0, 0, 0).to_zoned_datetime(tz.clone())?.timestamp())
}

fn open(cli: &Cli) -> Result<Store> {
    let Some(now) = &cli.now else {
        return Store::open(&cli.db);
    };
    // Resolve a bare date in the household's zone, if there is one yet.
    let tz = Store::open(&cli.db)
        .and_then(|s| s.household())
        .ok()
        .and_then(|h| TimeZone::get(&h.time_zone).ok())
        .unwrap_or(TimeZone::UTC);
    let t = parse_now(now, &tz)?;
    let clock = ManualClock::new(t.as_millisecond().max(0) as u64);
    Store::open_with_clock(&cli.db, Box::new(clock))
}

fn short(id: impl ToString) -> String {
    let s = id.to_string();
    format!("#{}", &s[s.len() - 8..])
}

/// Find one entity by `#suffix`, full id, or case-insensitive name.
fn resolve<E: Entity>(store: &Store, what: &str, name: impl Fn(&E) -> &str) -> Result<E> {
    let all: Vec<E> = store.list()?;
    let key = what.trim_start_matches('#').to_lowercase();
    let matches: Vec<E> = all
        .into_iter()
        .filter(|e| {
            let id = e.entity_id();
            (key.len() >= 4 && id.ends_with(&key)) || id == key || name(e).to_lowercase() == key
        })
        .collect();
    let kind = E::KIND.as_str();
    match matches.len() {
        0 => Err(Error::NotFound(format!("{kind} '{what}'"))),
        1 => Ok(matches.into_iter().next().expect("one")),
        n => Err(invalid(format!(
            "'{what}' matches {n} {kind}s; use the #id from a listing"
        ))),
    }
}

fn property_or_only(store: &Store, r: Option<&str>) -> Result<Property> {
    match r {
        Some(r) => resolve(store, r, |p: &Property| &p.name),
        None => {
            let mut all: Vec<Property> = store.list()?;
            match all.len() {
                1 => Ok(all.remove(0)),
                0 => Err(invalid("add a property first: hw property add --name Home")),
                _ => Err(invalid("several properties: pass --property")),
            }
        }
    }
}

fn today_in(store: &Store, tz_name: &str) -> Result<Date> {
    let tz = TimeZone::get(tz_name)?;
    Ok(housewright_core::schedule::civil_date(store.now(), &tz))
}

fn parse_value(t: &FieldType, raw: &str) -> Result<FieldValue> {
    Ok(match t {
        FieldType::Text => FieldValue::Text(raw.into()),
        FieldType::Number { unit } => FieldValue::Number {
            value: raw
                .parse()
                .map_err(|_| invalid(format!("'{raw}' is not a number")))?,
            unit: unit.clone(),
        },
        FieldType::Date => FieldValue::Date(
            raw.parse()
                .map_err(|e| invalid(format!("'{raw}' is not a date: {e}")))?,
        ),
        FieldType::YesNo => FieldValue::YesNo(match raw {
            "yes" | "true" | "y" => true,
            "no" | "false" | "n" => false,
            _ => return Err(invalid(format!("'{raw}' should be yes or no"))),
        }),
        FieldType::Choice { .. } => FieldValue::Choice(raw.into()),
        FieldType::Photo => return Err(invalid("photos cannot be set from the command line")),
    })
}

fn run(cli: Cli) -> Result<()> {
    if let Cmd::Templates { dir } = &cli.cmd {
        let lib = Library::load_dir(dir)?;
        for id in lib.ids() {
            let t = lib.resolve(id)?;
            println!("{id:<20} {:<30} {} tasks", t.name, t.tasks.len());
        }
        println!("{} templates OK", lib.len());
        return Ok(());
    }
    if let Cmd::Import { file } = &cli.cmd {
        let text = std::fs::read_to_string(file).map_err(|source| Error::Io {
            path: file.display().to_string(),
            source,
        })?;
        let e = export::from_json(&text)?;
        let mut store = open(&cli)?;
        let r = export::import(&mut store, &e, None)?;
        println!(
            "imported {} entities ({} operations, history {})",
            r.entities,
            r.operations,
            if r.with_history {
                "kept"
            } else {
                "started fresh"
            }
        );
        return Ok(());
    }

    let mut store = open(&cli)?;
    if let Cmd::Init {
        household,
        owner,
        tz,
    } = &cli.cmd
    {
        let (h, m) = store.init_household(household, owner, tz)?;
        println!(
            "created household '{}' in {}; you are {} (owner)",
            h.name,
            cli.db.display(),
            m.name
        );
        return Ok(());
    }
    let household = store.household()?;
    let me = store.author()?;
    let now = store.now();

    match cli.cmd {
        Cmd::Init { .. } | Cmd::Templates { .. } | Cmd::Import { .. } => unreachable!(),
        Cmd::Property(PropertyCmd::Add {
            name,
            tz,
            hemisphere,
            first_frost_doy,
            last_frost_doy,
        }) => {
            let tz = tz.unwrap_or(household.time_zone.clone());
            TimeZone::get(&tz).map_err(|e| invalid(format!("time zone '{tz}': {e}")))?;
            for d in [first_frost_doy, last_frost_doy].into_iter().flatten() {
                if !(1..=365).contains(&d) {
                    return Err(invalid("frost days of year are 1-365"));
                }
            }
            let p = Property {
                id: PropertyId::new(),
                name,
                address: None,
                time_zone: tz,
                climate: Climate {
                    hemisphere: match hemisphere {
                        Hemi::North => Hemisphere::North,
                        Hemi::South => Hemisphere::South,
                    },
                    first_frost_doy,
                    last_frost_doy,
                    hard_freeze: None,
                },
            };
            store.put(&p)?;
            println!("added property '{}' {}", p.name, short(p.id));
        }
        Cmd::Property(PropertyCmd::List) => {
            for p in store.list::<Property>()? {
                println!("{} {} ({})", short(p.id), p.name, p.time_zone);
            }
        }
        Cmd::Room(RoomCmd::Add { name, property }) => {
            let p = property_or_only(&store, property.as_deref())?;
            let r = Room {
                outline: None,
                id: RoomId::new(),
                property: p.id,
                floor: None,
                name,
            };
            store.put(&r)?;
            println!("added room '{}' {}", r.name, short(r.id));
        }
        Cmd::Room(RoomCmd::List) => {
            for r in store.list::<Room>()? {
                println!("{} {}", short(r.id), r.name);
            }
        }
        Cmd::Item(ItemCmd::Add {
            name,
            property,
            room,
            template,
            templates,
            make,
            model,
            serial,
            installed,
            set,
        }) => {
            let p = property_or_only(&store, property.as_deref())?;
            let room = match room {
                Some(r) => Some(resolve(&store, &r, |r: &Room| &r.name)?.id),
                None => None,
            };
            let resolved = match &template {
                Some(t) => Some(Library::load_dir(&templates)?.resolve(t)?),
                None => None,
            };
            let mut item = Item {
                id: ItemId::new(),
                property: p.id,
                room,
                parent: None,
                name,
                category: resolved.as_ref().and_then(|t| t.category.clone()),
                make,
                model,
                serial: serial.map(Flagged::normal),
                purchased: None,
                installed,
                warranty: None,
                location_note: None,
                template: template.clone(),
                extra_fields: vec![],
                custom: Default::default(),
                sensitivity: Sensitivity::Normal,
            };
            for kv in set {
                let (k, v) = kv
                    .split_once('=')
                    .ok_or_else(|| invalid(format!("--set '{kv}' should be key=value")))?;
                let def = resolved
                    .as_ref()
                    .and_then(|t| t.fields.iter().find(|f| f.key == k))
                    .ok_or_else(|| invalid(format!("no field '{k}' on this template")))?;
                let value = parse_value(&def.field_type, v)?;
                def.check(&value).map_err(invalid)?;
                item.custom.insert(
                    k.to_string(),
                    CustomValue {
                        value,
                        sensitivity: def.sensitivity,
                    },
                );
            }
            match resolved {
                Some(t) => {
                    let start = today_in(&store, &p.time_zone)?;
                    let tasks = store.add_item_from_template(&item, &t, me, start)?;
                    println!(
                        "added item '{}' {} with {} suggested tasks (run `hw start` to switch the top few on)",
                        item.name,
                        short(item.id),
                        tasks.len()
                    );
                }
                None => {
                    store.put(&item)?;
                    println!("added item '{}' {}", item.name, short(item.id));
                }
            }
        }
        Cmd::Item(ItemCmd::List) => {
            for i in store.list::<Item>()? {
                println!(
                    "{} {}{}",
                    short(i.id),
                    i.name,
                    i.template.map(|t| format!(" [{t}]")).unwrap_or_default()
                );
            }
        }
        Cmd::Task(TaskCmd::Add(a)) => {
            let (property, item) = match &a.item {
                Some(i) => {
                    let it = resolve(&store, i, |x: &Item| &x.name)?;
                    (
                        store.fetch::<Property>(&it.property.to_string())?,
                        Some(it.id),
                    )
                }
                None => (property_or_only(&store, a.property.as_deref())?, None),
            };
            let owner = match &a.owner {
                Some(o) => resolve(&store, o, |m: &Member| &m.name)?.id,
                None => me,
            };
            let start = today_in(&store, &property.time_zone)?;
            let schedule = if let Some(every) = a.every {
                Schedule::Fixed {
                    every,
                    anchor: None,
                }
            } else if let Some(every) = a.after {
                Schedule::Floating { every }
            } else if let Some(s) = a.season {
                Schedule::Seasonal {
                    season: match s {
                        SeasonArg::Spring => Season::Spring,
                        SeasonArg::Summer => Season::Summer,
                        SeasonArg::Autumn => Season::Autumn,
                        SeasonArg::Winter => Season::Winter,
                        SeasonArg::BeforeFirstFrost => Season::BeforeFirstFrost,
                        SeasonArg::AfterLastFrost => Season::AfterLastFrost,
                    },
                }
            } else if let Some(due) = a.once {
                Schedule::OneOff { due }
            } else {
                return Err(invalid("give one of --every, --after, --season or --once"));
            };
            let t = Task {
                id: TaskId::new(),
                property: property.id,
                item,
                room: None,
                title: a.title,
                owner,
                priority: match a.priority {
                    Prio::LifeSafety => PriorityClass::LifeSafety,
                    Prio::PropertyLoss => PriorityClass::PropertyLoss,
                    Prio::Efficiency => PriorityClass::Efficiency,
                    Prio::Cosmetic => PriorityClass::Cosmetic,
                },
                schedule,
                window: None,
                start,
                steps: vec![],
                why: None,
                minutes: None,
                completion_fields: vec![],
                template: None,
                active: true,
                paused: false,
                not_applicable: false,
            };
            store.put(&t)?;
            println!("added task '{}' {}", t.title, short(t.id));
        }
        Cmd::Task(TaskCmd::List { all }) => {
            let ranked = store.ranked_tasks(me, now, all)?;
            if ranked.is_empty() {
                println!("no tasks switched on (try `hw task list --all` or `hw start`)");
            }
            for (t, s) in ranked {
                let window = match s.eval.window {
                    Some(w) => format!(
                        "due {} (window {} .. {})",
                        w.due, w.earliest, w.overdue_after
                    ),
                    None => "no date yet".into(),
                };
                let mut flags = String::new();
                if !t.active {
                    flags.push_str(" [suggested]");
                }
                if s.eval.needs_review {
                    flags.push_str(" [snoozed often: review]");
                }
                println!(
                    "{} {:<10} {:<19} {:<55} {}{}",
                    short(t.id),
                    s.eval.state.label(),
                    t.priority.label(),
                    t.title,
                    window,
                    flags
                );
            }
        }
        Cmd::Task(TaskCmd::Done { task, note }) => {
            let t = resolve(&store, &task, |t: &Task| &t.title)?;
            store.complete_task(t.id, None, note.as_deref())?;
            let s = store.task_status(&t, now)?;
            let next = s
                .eval
                .window
                .map(|w| format!("; next due {}", w.due))
                .unwrap_or_default();
            println!("done: {}{next}", t.title);
        }
        Cmd::Task(TaskCmd::Snooze { task, until }) => {
            let t = resolve(&store, &task, |t: &Task| &t.title)?;
            store.snooze_task(t.id, until, None)?;
            let s = store.task_status(&t, now)?;
            println!("snoozed '{}' until {until}", t.title);
            if s.eval.needs_review {
                println!(
                    "This has been snoozed {} times. Consider changing how often it repeats, \
                     handing it to someone else, marking it not applicable, or finding a pro.",
                    s.eval.snooze_count
                );
            }
        }
        Cmd::Task(TaskCmd::On { task }) => {
            let mut t = resolve(&store, &task, |t: &Task| &t.title)?;
            t.active = true;
            store.put(&t)?;
            println!("switched on '{}'", t.title);
        }
        Cmd::Task(TaskCmd::Show { task }) => {
            let t = resolve(&store, &task, |t: &Task| &t.title)?;
            let s = store.task_status(&t, now)?;
            println!("{} {}", short(t.id), t.title);
            println!(
                "  {} | {} | {}",
                t.priority.label(),
                t.schedule.describe(),
                s.eval.state.label()
            );
            println!("  owner: {}", store.member_name(t.owner));
            if let Some(w) = &t.why {
                println!("  why: {} (source: {})", w.text, w.source);
            }
            for (i, step) in t.steps.iter().enumerate() {
                println!("  {}. {step}", i + 1);
            }
        }
        Cmd::Start { n } => {
            let picked = store.start_small(n, now)?;
            for id in &picked {
                let t: Task = store.fetch(&id.to_string())?;
                println!("switched on: {} ({})", t.title, t.priority.label());
            }
            if picked.is_empty() {
                println!("nothing to switch on");
            }
        }
        Cmd::WelcomeBack => {
            let statuses: Vec<_> = store
                .ranked_tasks(me, now, false)?
                .into_iter()
                .map(|(_, s)| s)
                .collect();
            let wb = welcome_back(&statuses);
            if wb.focus.is_empty() {
                println!("Welcome back. Nothing is overdue.");
            } else {
                println!("Welcome back. These matter most:");
                for s in wb.focus {
                    println!("  {} {} ({})", short(s.task), s.title, s.priority.label());
                }
            }
        }
        Cmd::Audit => {
            let tz = TimeZone::get(&household.time_zone)?;
            for e in audit_log(&store, &tz)? {
                let when = tz.to_datetime(e.at);
                println!("{} {}", when.strftime("%Y-%m-%d %H:%M"), e.text);
            }
        }
        Cmd::Export { file } => {
            let e = export::export(&store)?;
            std::fs::write(&file, export::to_json(&e)?).map_err(|source| Error::Io {
                path: file.display().to_string(),
                source,
            })?;
            println!(
                "exported {} entities and {} operations to {}",
                e.entities.values().map(Vec::len).sum::<usize>(),
                e.operations.len(),
                file.display()
            );
        }
    }
    Ok(())
}
