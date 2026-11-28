//! Just-in-time growth (P3, D-67): at most one new task or question a week.
//!
//! Pure. Given the profile, the things already recorded, the suggested
//! (switched-off) tasks, earlier answers, the climate and today, propose the
//! single most useful next step, or nothing:
//!
//! 1. Urgent life-safety gaps first: no smoke alarm, or no CO alarm where
//!    something burns fuel.
//! 2. Then whatever the season makes relevant now (a seasonal task or
//!    question whose window opens within [`SEASON_LEAD_DAYS`] or is open),
//!    most important class first.
//!    Questions about checklist items answered "Not sure" come after the
//!    other seasonal suggestions.
//! 3. Then everything else, most important class first.
//!
//! Rules: nothing at all if any suggestion was answered in the last
//! [`WEEK_DAYS`] days; "Not for us" and "Add" are permanent for that
//! suggestion; "Later" hides it for [`LATER_DAYS`] days. In a climate where
//! hard freezes are not normal, frost tasks and questions are never proposed.

use std::collections::BTreeMap;

use jiff::civil::Date;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

use crate::ids::{MemberId, PropertyId, SuggestionAnswerId, TaskId};
use crate::model::Task;
use crate::priority::{PriorityClass, TaskStatus};
use crate::profile::{AlarmCount, Feature, HomeType, Profile};
use crate::schedule::{Climate, Season, season_range};

pub const WEEK_DAYS: i64 = 7;
pub const LATER_DAYS: i64 = 28;
/// A seasonal suggestion is relevant from this many days before its window.
pub const SEASON_LEAD_DAYS: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Yes: add the thing, or switch the task on.
    Added,
    /// Never suggest this again.
    NotForUs,
    /// Ask again in a few weeks.
    Later,
}

/// Append-only record of an answer to a suggestion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SuggestionAnswer {
    pub id: SuggestionAnswerId,
    pub property: PropertyId,
    /// The suggestion's key (`ask:hose-bibs`, `task:<id>`).
    pub key: String,
    pub answer: Answer,
    pub by: MemberId,
    pub at: Timestamp,
}
crate::entity!(SuggestionAnswer, SuggestionAnswer);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SuggestionAction {
    /// Add a thing from this template (and switch on its most relevant task).
    AddThing {
        template: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        season: Option<Season>,
    },
    /// Switch on a suggested task.
    SwitchOn { task: TaskId },
}

/// One proposal, ready for a card on Today.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub key: String,
    pub action: SuggestionAction,
    /// The question or the task ("Do you have outdoor taps?").
    pub title: String,
    /// One or two plain sentences: what and when.
    pub body: String,
    pub why: String,
    pub source: String,
    /// Label for the "yes" button.
    pub add_label: String,
    pub priority: PriorityClass,
    /// True when it is proposed now because of the season.
    pub seasonal: bool,
}

/// A seasonal task that is switched off, with what the card needs.
#[derive(Clone, Debug)]
pub struct Dormant<'a> {
    pub task: &'a Task,
    pub status: &'a TaskStatus,
    pub item_name: Option<&'a str>,
}

pub struct SuggestInput<'a> {
    pub profile: Option<&'a Profile>,
    pub climate: &'a Climate,
    /// Template ids of things already recorded.
    pub templates_present: &'a [String],
    /// Suggested (switched-off) tasks.
    pub dormant: &'a [Dormant<'a>],
    pub answers: &'a [SuggestionAnswer],
    pub today: Date,
    pub now: Timestamp,
}

fn add_days(d: Date, n: i64) -> Date {
    d.checked_add(Span::new().days(n)).unwrap_or(d)
}

fn is_frost(s: Season) -> bool {
    matches!(s, Season::BeforeFirstFrost | Season::AfterLastFrost)
}

/// Whether `season` is open, or opens within [`SEASON_LEAD_DAYS`], on `today`.
pub fn in_season(season: Season, climate: &Climate, today: Date) -> bool {
    let y = today.year();
    [y - 1, y, y + 1].into_iter().any(|year| {
        season_range(season, climate, year)
            .is_some_and(|(a, b)| add_days(a, -SEASON_LEAD_DAYS) <= today && today <= b)
    })
}

/// "around Nov 17" for a frost season, from the climate data.
fn frost_phrase(season: Season, climate: &Climate) -> Option<String> {
    let doy = match season {
        Season::BeforeFirstFrost => climate.first_frost_doy?,
        Season::AfterLastFrost => climate.last_frost_doy?,
        _ => return None,
    };
    let d = Date::new(2025, 1, 1)
        .ok()?
        .checked_add(Span::new().days(i64::from(doy) - 1))
        .ok()?;
    Some(d.strftime("%b %-d").to_string())
}

struct Question {
    template: &'static str,
    name: &'static str,
    title: &'static str,
    body: &'static str,
    why: &'static str,
    source: &'static str,
    priority: PriorityClass,
    season: Option<Season>,
}

const HOSE_TAPS: Question = Question {
    template: "hose-bibs",
    name: "Outdoor hose taps",
    title: "Do you have outdoor taps?",
    body: "If so, winterise them before the first frost",
    why: "Water left in an outdoor tap's pipe can freeze, split it and leak inside the wall.",
    source: "common plumber guidance",
    priority: PriorityClass::PropertyLoss,
    season: Some(Season::BeforeFirstFrost),
};

const EXTINGUISHER: Question = Question {
    template: "fire-extinguisher",
    name: "Fire extinguisher",
    title: "Do you have a fire extinguisher?",
    body: "If so, a quick monthly look at its gauge keeps it ready.",
    why: "An extinguisher that has lost pressure will not work when a small fire starts.",
    source: "NFPA guidance",
    priority: PriorityClass::LifeSafety,
    season: None,
};

const GARAGE_DOOR: Question = Question {
    template: "garage-door-opener",
    name: "Garage door opener",
    title: "Does your garage have an electric door opener?",
    body: "If so, test its safety reverse once a month. It takes two minutes.",
    why: "The safety reverse stops a closing door from crushing a child, a pet or a car.",
    source: "CPSC guidance",
    priority: PriorityClass::LifeSafety,
    season: None,
};

const SPRINKLERS: Question = Question {
    template: "irrigation",
    name: "Lawn sprinkler system",
    title: "Do you have a lawn sprinkler system?",
    body: "If so, start it up and check the heads once the frosts are over.",
    why: "A head or pipe that cracked over winter wastes water and can wash out the yard.",
    source: "common irrigation contractor guidance",
    priority: PriorityClass::Efficiency,
    season: Some(Season::AfterLastFrost),
};

/// A checklist item answered "Not sure": ask about it again, one at a time.
fn checklist_question(f: Feature) -> Option<(&'static str, PriorityClass, Option<Season>)> {
    Some(match f {
        Feature::SumpPump => (
            "Do you have a sump pump?",
            PriorityClass::PropertyLoss,
            Some(Season::Spring),
        ),
        Feature::Gutters => (
            "Does your home have gutters?",
            PriorityClass::PropertyLoss,
            Some(Season::Autumn),
        ),
        Feature::Fireplace => (
            "Do you have a fireplace?",
            PriorityClass::LifeSafety,
            Some(Season::Autumn),
        ),
        Feature::WoodStove => (
            "Do you have a wood stove?",
            PriorityClass::LifeSafety,
            Some(Season::Autumn),
        ),
        Feature::Dryer => (
            "Do you have a clothes dryer?",
            PriorityClass::PropertyLoss,
            None,
        ),
        Feature::Washer => (
            "Do you have a washing machine?",
            PriorityClass::PropertyLoss,
            None,
        ),
        Feature::Dehumidifier => (
            "Do you have a dehumidifier?",
            PriorityClass::Efficiency,
            Some(Season::Summer),
        ),
        Feature::Attic => (
            "Do you have an attic you can get into?",
            PriorityClass::PropertyLoss,
            Some(Season::Autumn),
        ),
        Feature::Crawlspace => (
            "Do you have a crawlspace?",
            PriorityClass::PropertyLoss,
            Some(Season::Spring),
        ),
        Feature::Dishwasher => (
            "Do you have a dishwasher?",
            PriorityClass::PropertyLoss,
            None,
        ),
        Feature::FridgeWater => (
            "Does your fridge have a water or ice dispenser?",
            PriorityClass::Efficiency,
            None,
        ),
        Feature::RangeHood => (
            "Do you have a range hood over the stove?",
            PriorityClass::PropertyLoss,
            None,
        ),
        Feature::Disposal => (
            "Do you have a garbage disposal?",
            PriorityClass::Efficiency,
            None,
        ),
        Feature::Deck => (
            "Do you have a deck?",
            PriorityClass::LifeSafety,
            Some(Season::Spring),
        ),
        Feature::Pool => (
            "Do you have a swimming pool?",
            PriorityClass::LifeSafety,
            Some(Season::Spring),
        ),
        Feature::Yard => (
            "Do you have a lawn to mow?",
            PriorityClass::Cosmetic,
            Some(Season::Spring),
        ),
        Feature::WaterSoftener => (
            "Do you have a water softener?",
            PriorityClass::Efficiency,
            None,
        ),
        Feature::Generator => ("Do you have a generator?", PriorityClass::LifeSafety, None),
        Feature::Solar => ("Do you have solar panels?", PriorityClass::Efficiency, None),
        Feature::Basement | Feature::Garage => return None,
    })
}

struct Ranked {
    /// 2 = no smoke alarm, 1 = no CO alarm, 0 = not urgent.
    urgent: u8,
    /// Asking again about a checklist item answered "Not sure".
    reask: bool,
    seasonal: bool,
    s: Suggestion,
}

fn ask(q: &Question, climate: &Climate, seasonal: bool) -> Suggestion {
    let body = match q.season.and_then(|s| frost_phrase(s, climate)) {
        Some(when) => format!("{} (on average around {when} where you live).", q.body),
        None if q.body.ends_with('.') => q.body.to_string(),
        None => format!("{}.", q.body),
    };
    Suggestion {
        key: format!("ask:{}", q.template),
        action: SuggestionAction::AddThing {
            template: q.template.into(),
            name: q.name.into(),
            season: q.season,
        },
        title: q.title.into(),
        body,
        why: q.why.into(),
        source: q.source.into(),
        add_label: "Yes, add it".into(),
        priority: q.priority,
        seasonal,
    }
}

/// Every eligible suggestion, best first, ignoring the once-a-week limit.
pub fn candidates(input: &SuggestInput<'_>) -> Vec<Suggestion> {
    let now_s = input.now.as_second();
    let mut answered: BTreeMap<&str, (Answer, i64)> = BTreeMap::new();
    let mut sorted: Vec<&SuggestionAnswer> = input.answers.iter().collect();
    sorted.sort_by_key(|a| a.at);
    for a in sorted {
        // A permanent answer is never overridden by a later "Later".
        let keep = matches!(
            answered.get(a.key.as_str()),
            Some((Answer::Added | Answer::NotForUs, _))
        );
        if !keep {
            answered.insert(a.key.as_str(), (a.answer, a.at.as_second()));
        }
    }
    let blocked = |key: &str| match answered.get(key) {
        Some((Answer::Added | Answer::NotForUs, _)) => true,
        Some((Answer::Later, at)) => now_s - at < LATER_DAYS * 86_400,
        None => false,
    };
    let present = |t: &str| input.templates_present.iter().any(|p| p == t);
    let freezes = input.climate.freezes();
    let relevant = |season: Option<Season>| match season {
        Some(s) if is_frost(s) && !freezes => None,
        Some(s) => Some(in_season(s, input.climate, input.today)),
        None => Some(false),
    };

    let mut out: Vec<Ranked> = Vec::new();
    let p = input.profile;

    // 1. Urgent gaps.
    if !present("smoke-alarm") {
        let none = p.is_some_and(|p| p.smoke_alarms == Some(AlarmCount::None));
        out.push(Ranked {
            urgent: 2,
            reask: false,
            seasonal: false,
            s: Suggestion {
                key: "ask:smoke-alarm".into(),
                action: SuggestionAction::AddThing {
                    template: "smoke-alarm".into(),
                    name: "Smoke alarms".into(),
                    season: None,
                },
                title: if none {
                    "Fit smoke alarms".into()
                } else {
                    "Do you have smoke alarms?".into()
                },
                body: if none {
                    "Fit one on every level and outside each sleeping area. When they are up, add them here for a monthly test.".into()
                } else {
                    "If so, add them here and Housewright will remind you to test them once a month.".into()
                },
                why: "A working smoke alarm gives you the minutes you need to get out in a fire.".into(),
                source: "NFPA guidance".into(),
                add_label: if none {
                    "They are fitted".into()
                } else {
                    "Yes, add them".into()
                },
                priority: PriorityClass::LifeSafety,
                seasonal: false,
            },
        });
    }
    if !present("co-alarm") && p.is_some_and(Profile::burns_fuel) {
        let none = p.is_some_and(|p| p.co_alarms == Some(AlarmCount::None));
        out.push(Ranked {
            urgent: 1,
            reask: false,
            seasonal: false,
            s: Suggestion {
                key: "ask:co-alarm".into(),
                action: SuggestionAction::AddThing {
                    template: "co-alarm".into(),
                    name: "CO alarms".into(),
                    season: None,
                },
                title: if none {
                    "Fit a carbon monoxide alarm".into()
                } else {
                    "Do you have a carbon monoxide alarm?".into()
                },
                body: "Something in your home burns fuel, so a CO alarm near the bedrooms matters. Add it here for a monthly test.".into(),
                why: "Carbon monoxide has no smell; an alarm is the only way to know it is there.".into(),
                source: "US Fire Administration guidance".into(),
                add_label: if none {
                    "It is fitted".into()
                } else {
                    "Yes, add it".into()
                },
                priority: PriorityClass::LifeSafety,
                seasonal: false,
            },
        });
    }

    // 2 and 3. Questions about things not yet recorded.
    let has_outside = p
        .and_then(|p| p.home_type)
        .is_none_or(HomeType::has_exterior);
    let mut questions: Vec<&Question> = Vec::new();
    if has_outside {
        questions.push(&HOSE_TAPS);
    }
    questions.push(&EXTINGUISHER);
    if p.is_some_and(|p| p.has(Feature::Garage) == Some(true)) {
        questions.push(&GARAGE_DOOR);
    }
    if p.is_some_and(|p| p.has(Feature::Yard) == Some(true)) {
        questions.push(&SPRINKLERS);
    }
    for q in questions {
        if present(q.template) {
            continue;
        }
        if let Some(seasonal) = relevant(q.season) {
            // Seasonal questions wait for their season.
            if q.season.is_some() && !seasonal {
                continue;
            }
            out.push(Ranked {
                urgent: 0,
                reask: false,
                seasonal,
                s: ask(q, input.climate, seasonal),
            });
        }
    }
    if let Some(p) = p {
        for f in Feature::ALL {
            if p.has(f).is_some() {
                continue;
            }
            let (Some((template, name, _)), Some((title, priority, season))) =
                (f.thing(), checklist_question(f))
            else {
                continue;
            };
            if present(template) {
                continue;
            }
            let Some(seasonal) = relevant(season) else {
                continue;
            };
            let q = Question {
                template,
                name,
                title,
                body: "If so, add it and Housewright will suggest a small plan for it",
                why: "Things you have recorded get reminders; things you have not are easy to forget.",
                source: "Housewright",
                priority,
                season,
            };
            out.push(Ranked {
                urgent: 0,
                reask: true,
                seasonal,
                s: ask(&q, input.climate, seasonal),
            });
        }
    }

    // Switched-off tasks.
    for d in input.dormant {
        if !d.status.eval.state.is_live() {
            continue;
        }
        let Some(seasonal) = relevant(d.task.schedule.season()) else {
            continue;
        };
        let season = d.task.schedule.season();
        let when = match season.and_then(|s| frost_phrase(s, input.climate)) {
            Some(w) if season == Some(Season::BeforeFirstFrost) => {
                format!(" Best done before the first frost, around {w} where you live.")
            }
            Some(w) => format!(" Best done after the last frost, around {w} where you live."),
            None if seasonal => " This is the time of year for it.".to_string(),
            None => String::new(),
        };
        let what = match d.item_name {
            Some(n) => format!("A suggested task for your {}.", n.to_lowercase()),
            None => "A suggested task.".to_string(),
        };
        let why = d.task.why.as_ref();
        out.push(Ranked {
            urgent: 0,
            reask: false,
            seasonal,
            s: Suggestion {
                key: format!("task:{}", d.task.id),
                action: SuggestionAction::SwitchOn { task: d.task.id },
                title: d.task.title.clone(),
                body: format!("{what}{when}"),
                why: why.map(|w| w.text.clone()).unwrap_or_default(),
                source: why.map(|w| w.source.clone()).unwrap_or_default(),
                add_label: "Switch it on".into(),
                priority: d.task.priority,
                seasonal,
            },
        });
    }

    out.retain(|r| !blocked(&r.s.key));
    out.sort_by(|a, b| {
        b.urgent
            .cmp(&a.urgent)
            .then(b.seasonal.cmp(&a.seasonal))
            .then(a.reask.cmp(&b.reask))
            .then(a.s.priority.cmp(&b.s.priority))
            // Questions about new things before switching on more tasks for
            // things already covered, so the plan widens before it deepens.
            .then(
                matches!(a.s.action, SuggestionAction::SwitchOn { .. })
                    .cmp(&matches!(b.s.action, SuggestionAction::SwitchOn { .. })),
            )
            .then(a.s.key.cmp(&b.s.key))
    });
    out.into_iter().map(|r| r.s).collect()
}

/// Whether any suggestion was answered in the last week.
pub fn answered_this_week(answers: &[SuggestionAnswer], now: Timestamp) -> bool {
    answers
        .iter()
        .any(|a| now.as_second() - a.at.as_second() < WEEK_DAYS * 86_400)
}

/// The one suggestion for this week, if any.
pub fn propose(input: &SuggestInput<'_>) -> Option<Suggestion> {
    if answered_this_week(input.answers, input.now) {
        return None;
    }
    candidates(input).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Why;
    use crate::profile::Heating;
    use crate::schedule::{Evaluation, Schedule, TaskState};
    use jiff::civil::date;

    fn ts(d: Date) -> Timestamp {
        jiff::tz::TimeZone::UTC
            .to_timestamp(d.at(12, 0, 0, 0))
            .unwrap()
    }

    fn philly() -> Climate {
        Climate {
            first_frost_doy: Some(321),
            last_frost_doy: Some(89),
            hard_freeze: Some(true),
            ..Climate::default()
        }
    }

    fn phoenix() -> Climate {
        Climate {
            first_frost_doy: Some(3),
            last_frost_doy: Some(5),
            hard_freeze: Some(false),
            ..Climate::default()
        }
    }

    fn profile() -> Profile {
        let mut p = Profile::empty(PropertyId::new());
        p.home_type = Some(HomeType::House);
        p
    }

    fn input<'a>(
        p: &'a Profile,
        climate: &'a Climate,
        present: &'a [String],
        dormant: &'a [Dormant<'a>],
        answers: &'a [SuggestionAnswer],
        today: Date,
    ) -> SuggestInput<'a> {
        SuggestInput {
            profile: Some(p),
            climate,
            templates_present: present,
            dormant,
            answers,
            today,
            now: ts(today),
        }
    }

    fn alarms() -> Vec<String> {
        vec!["smoke-alarm".into(), "co-alarm".into()]
    }

    fn answer(key: &str, a: Answer, on: Date) -> SuggestionAnswer {
        SuggestionAnswer {
            id: SuggestionAnswerId::new(),
            property: PropertyId::new(),
            key: key.into(),
            answer: a,
            by: MemberId::new(),
            at: ts(on),
        }
    }

    #[test]
    fn october_in_philadelphia_asks_about_outdoor_taps() {
        let p = profile();
        let c = philly();
        let present = alarms();
        let today = date(2026, 10, 12);
        let s = propose(&input(&p, &c, &present, &[], &[], today)).unwrap();
        assert_eq!(s.key, "ask:hose-bibs");
        assert!(s.seasonal);
        assert!(s.body.contains("Nov 17"), "{}", s.body);
        // In spring it is not the season, so it is not proposed.
        let spring = candidates(&input(&p, &c, &present, &[], &[], date(2026, 4, 20)));
        assert!(spring.iter().all(|s| s.key != "ask:hose-bibs"));
    }

    #[test]
    fn phoenix_never_asks_about_winterising() {
        let p = profile();
        let c = phoenix();
        let present = alarms();
        for m in 1..=12 {
            let all = candidates(&input(&p, &c, &present, &[], &[], date(2026, m, 15)));
            assert!(all.iter().all(|s| s.key != "ask:hose-bibs"), "month {m}");
        }
    }

    #[test]
    fn real_lookups_drive_the_hose_tap_question() {
        let p = profile();
        let present = alarms();
        let today = date(2026, 10, 12);
        let philly = crate::climate::lookup_us_zip("19103").unwrap().climate;
        let phoenix = crate::climate::lookup_us_zip("85004").unwrap().climate;
        let a = candidates(&input(&p, &philly, &present, &[], &[], today));
        assert!(a.iter().any(|s| s.key == "ask:hose-bibs"));
        let b = candidates(&input(&p, &phoenix, &present, &[], &[], today));
        assert!(b.iter().all(|s| s.key != "ask:hose-bibs"));
    }

    #[test]
    fn missing_alarms_come_first_whatever_the_season() {
        let mut p = profile();
        p.smoke_alarms = Some(AlarmCount::None);
        p.heating = Some(vec![Heating::GasFurnace]);
        let c = philly();
        let today = date(2026, 10, 12);
        let all = candidates(&input(&p, &c, &[], &[], &[], today));
        assert_eq!(all[0].key, "ask:smoke-alarm");
        assert_eq!(all[0].title, "Fit smoke alarms");
        assert_eq!(all[1].key, "ask:co-alarm");
        // No fuel burned: no CO question.
        let q = profile();
        let all = candidates(&input(&q, &c, &[], &[], &[], today));
        assert!(all.iter().all(|s| s.key != "ask:co-alarm"));
    }

    #[test]
    fn one_a_week_and_not_for_us_is_permanent() {
        let p = profile();
        let c = philly();
        let present = alarms();
        let today = date(2026, 10, 12);
        // Answered something three days ago: nothing this week.
        let recent = [answer(
            "ask:fire-extinguisher",
            Answer::Added,
            date(2026, 10, 9),
        )];
        assert!(propose(&input(&p, &c, &present, &[], &recent, today)).is_none());
        // Eight days later the next one comes.
        let later = date(2026, 10, 17);
        assert!(propose(&input(&p, &c, &present, &[], &recent, later)).is_some());

        let no = [answer("ask:hose-bibs", Answer::NotForUs, date(2025, 10, 1))];
        for year in [2026, 2027, 2030] {
            let all = candidates(&input(&p, &c, &present, &[], &no, date(year, 10, 12)));
            assert!(all.iter().all(|s| s.key != "ask:hose-bibs"), "{year}");
        }
        // A "Later" afterwards does not undo "Not for us".
        let both = [
            answer("ask:hose-bibs", Answer::NotForUs, date(2025, 10, 1)),
            answer("ask:hose-bibs", Answer::Later, date(2025, 10, 20)),
        ];
        let all = candidates(&input(&p, &c, &present, &[], &both, date(2026, 10, 12)));
        assert!(all.iter().all(|s| s.key != "ask:hose-bibs"));
    }

    #[test]
    fn later_comes_back_after_four_weeks() {
        let p = profile();
        let c = philly();
        let present = alarms();
        let a = [answer(
            "ask:fire-extinguisher",
            Answer::Later,
            date(2026, 1, 1),
        )];
        let soon = candidates(&input(&p, &c, &present, &[], &a, date(2026, 1, 20)));
        assert!(soon.iter().all(|s| s.key != "ask:fire-extinguisher"));
        let after = candidates(&input(&p, &c, &present, &[], &a, date(2026, 2, 5)));
        assert!(after.iter().any(|s| s.key == "ask:fire-extinguisher"));
    }

    fn task(title: &str, schedule: Schedule, p: PriorityClass) -> Task {
        Task {
            id: TaskId::new(),
            property: PropertyId::new(),
            item: None,
            room: None,
            title: title.into(),
            owner: MemberId::new(),
            priority: p,
            schedule,
            window: None,
            start: date(2026, 1, 1),
            steps: vec![],
            why: Some(Why {
                text: "because".into(),
                source: "test".into(),
                link: None,
            }),
            minutes: None,
            completion_fields: vec![],
            template: None,
            active: false,
            paused: false,
            not_applicable: false,
            guidance: None,
        }
    }

    fn status(t: &Task, state: TaskState) -> TaskStatus {
        TaskStatus {
            task: t.id,
            title: t.title.clone(),
            priority: t.priority,
            eval: Evaluation {
                state,
                window: None,
                last_done: None,
                snooze_count: 0,
                needs_review: false,
                usage: None,
            },
        }
    }

    #[test]
    fn seasonal_dormant_task_beats_a_question_out_of_season() {
        let p = profile();
        let c = philly();
        let mut present = alarms();
        present.push("hose-bibs".into());
        present.push("fire-extinguisher".into());
        let gutters = task(
            "Clean the gutters",
            Schedule::Seasonal {
                season: Season::Autumn,
            },
            PriorityClass::PropertyLoss,
        );
        let gutters_s = status(&gutters, TaskState::Due);
        let filter = task(
            "Replace the filter",
            Schedule::Floating {
                every: crate::schedule::Interval::months(3),
            },
            PriorityClass::Efficiency,
        );
        let filter_s = status(&filter, TaskState::Due);
        let dead = task(
            "Not applicable",
            Schedule::Floating {
                every: crate::schedule::Interval::months(3),
            },
            PriorityClass::LifeSafety,
        );
        let dead_s = status(&dead, TaskState::NotApplicable);
        let dormant = [
            Dormant {
                task: &filter,
                status: &filter_s,
                item_name: None,
            },
            Dormant {
                task: &gutters,
                status: &gutters_s,
                item_name: Some("Gutters"),
            },
            Dormant {
                task: &dead,
                status: &dead_s,
                item_name: None,
            },
        ];
        let all = candidates(&input(&p, &c, &present, &dormant, &[], date(2026, 10, 12)));
        assert_eq!(all[0].title, "Clean the gutters");
        assert!(all[0].body.contains("your gutters"));
        assert!(all.iter().any(|s| s.title == "Replace the filter"));
        assert!(all.iter().all(|s| s.title != "Not applicable"));
        // Answering "Not for us" to the task removes it.
        let no = [answer(
            &format!("task:{}", gutters.id),
            Answer::NotForUs,
            date(2026, 1, 1),
        )];
        let all = candidates(&input(&p, &c, &present, &dormant, &no, date(2026, 10, 12)));
        assert_ne!(all[0].title, "Clean the gutters");
    }

    #[test]
    fn frost_tasks_are_skipped_where_it_does_not_freeze() {
        let p = profile();
        let c = phoenix();
        let present = alarms();
        let t = task(
            "Winterise the sprinklers",
            Schedule::Seasonal {
                season: Season::BeforeFirstFrost,
            },
            PriorityClass::PropertyLoss,
        );
        let s = status(&t, TaskState::Scheduled);
        let dormant = [Dormant {
            task: &t,
            status: &s,
            item_name: None,
        }];
        let all = candidates(&input(&p, &c, &present, &dormant, &[], date(2026, 12, 1)));
        assert!(all.iter().all(|s| s.title != "Winterise the sprinklers"));
    }

    #[test]
    fn in_season_handles_windows_that_cross_new_year() {
        // Phoenix's average first frost is Jan 3: the window runs Nov 22 - Jan 2.
        let c = Climate {
            first_frost_doy: Some(3),
            ..Climate::default()
        };
        assert!(in_season(Season::BeforeFirstFrost, &c, date(2026, 12, 20)));
        assert!(in_season(Season::BeforeFirstFrost, &c, date(2027, 1, 2)));
        assert!(!in_season(Season::BeforeFirstFrost, &c, date(2026, 7, 1)));
        assert!(in_season(Season::Winter, &c, date(2027, 1, 15)));
    }
}
