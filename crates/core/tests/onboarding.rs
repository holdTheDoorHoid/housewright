//! Phase 3 end to end through the store: questionnaire answers become things
//! and switched-off tasks, a small starting plan is switched on (life safety
//! first), answering again only adds, the weekly suggestion is answered, and
//! exports from before Phase 3 still import.

use std::path::PathBuf;

use housewright_core::export;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::priority::PriorityClass;
use housewright_core::profile::*;
use housewright_core::schedule::{Climate, Hemisphere};
use housewright_core::store::{ManualClock, Store, WallClock};
use housewright_core::suggest::Answer;
use housewright_core::template::Library;
use jiff::Timestamp;
use jiff::civil::date;

/// 2026-11-27 12:00 UTC.
const NOW_MS: u64 = 1_795_780_800_000;
const DAY_MS: u64 = 86_400_000;

fn lib() -> Library {
    Library::load_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates")).unwrap()
}

struct Home {
    store: Store,
    clock: ManualClock,
    owner: MemberId,
    property: PropertyId,
}

impl Home {
    fn now(&self) -> Timestamp {
        Timestamp::from_millisecond(self.clock.now_ms() as i64).unwrap()
    }
}

fn home() -> Home {
    let clock = ManualClock::new(NOW_MS);
    let mut store = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    let (_, owner) = store
        .init_household("The Testers", "Alex", "America/New_York")
        .unwrap();
    let property = Property {
        id: PropertyId::new(),
        name: "Home".into(),
        address: None,
        time_zone: "America/New_York".into(),
        climate: Climate::default(),
    };
    store.put(&property).unwrap();
    Home {
        store,
        clock,
        owner: owner.id,
        property: property.id,
    }
}

fn philly_house(property: PropertyId) -> Profile {
    let mut p = Profile::empty(property);
    p.home_type = Some(HomeType::House);
    p.built = Some(BuiltEra::Before1950);
    p.zip = Some(Flagged::normal("19103".into()));
    p.heating = Some(vec![Heating::GasFurnace]);
    p.cooling = Some(vec![Cooling::CentralAc]);
    p.water_heater = Some(WaterHeaterKind::TankGas);
    p.water_supply = Some(WaterSupply::City);
    p.sewer = Some(Sewer::City);
    p.smoke_alarms = Some(AlarmCount::TwoToThree);
    p.smoke_power = Some(AlarmPower::Battery);
    p.co_alarms = Some(AlarmCount::One);
    for f in [
        Feature::Basement,
        Feature::SumpPump,
        Feature::Gutters,
        Feature::Dryer,
        Feature::Washer,
        Feature::Dishwasher,
    ] {
        p.features.insert(f, true);
    }
    p.features.insert(Feature::Pool, false);
    p.people.adults = Some(2);
    p
}

fn active(store: &Store) -> Vec<Task> {
    store
        .list::<Task>()
        .unwrap()
        .into_iter()
        .filter(|t| t.active)
        .collect()
}

#[test]
fn questionnaire_creates_things_with_everything_switched_off() {
    let mut h = home();
    let p = philly_house(h.property);
    let out = h
        .store
        .apply_profile(&p, &lib(), h.owner, date(2026, 11, 27))
        .unwrap();
    let items: Vec<Item> = h.store.list().unwrap();
    assert_eq!(items.len(), out.new_items.len());
    assert!(items.len() >= 15, "{} things", items.len());
    let templates: Vec<&str> = items.iter().filter_map(|i| i.template.as_deref()).collect();
    for want in [
        "smoke-alarm",
        "co-alarm",
        "furnace",
        "water-heater",
        "sump-pump",
        "roof",
    ] {
        assert!(templates.contains(&want), "missing {want}");
    }
    assert!(!templates.contains(&"pool"));
    assert!(
        active(&h.store).is_empty(),
        "everything starts switched off"
    );
    // Rooms where obvious.
    let rooms: Vec<Room> = h.store.list().unwrap();
    let names: Vec<&str> = rooms.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Basement") && names.contains(&"Laundry") && names.contains(&"Kitchen")
    );
    let wh = items
        .iter()
        .find(|i| i.template.as_deref() == Some("water-heater"))
        .unwrap();
    assert_eq!(
        rooms.iter().find(|r| Some(r.id) == wh.room).unwrap().name,
        "Basement"
    );
    // The gas water heater got its gas-only flue task.
    let wh_tasks: Vec<Task> = h.store.list_by("item", &wh.id.to_string()).unwrap();
    assert!(wh_tasks.iter().any(|t| t.title.contains("flue")));
    // The ZIP set the climate.
    let prop: Property = h.store.fetch(&h.property.to_string()).unwrap();
    assert!(prop.climate.first_frost_doy.is_some());
    assert_eq!(prop.climate.hard_freeze, Some(true));
    assert_eq!(prop.climate.hemisphere, Hemisphere::North);
    assert_eq!(out.climate.unwrap().matched_zip, "19103");
    // The profile is stored.
    assert_eq!(h.store.profile(h.property).unwrap().unwrap(), p);
}

#[test]
fn starting_plan_is_small_and_life_safety_first() {
    let mut h = home();
    let p = philly_house(h.property);
    h.store
        .apply_profile(&p, &lib(), h.owner, date(2026, 11, 27))
        .unwrap();
    let n = starting_count(&p);
    let picked = h
        .store
        .switch_on_starting_plan(h.property, n, h.now())
        .unwrap();
    assert_eq!(picked.len(), 4);
    let tasks: Vec<Task> = picked
        .iter()
        .map(|id| h.store.fetch(&id.to_string()).unwrap())
        .collect();
    let titles: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
    // The alarm tests lead, then life safety, then one water/fire task.
    assert_eq!(titles[0], "Test the CO alarm", "{titles:?}");
    assert_eq!(titles[1], "Test the smoke alarm", "{titles:?}");
    assert_eq!(tasks[2].priority, PriorityClass::LifeSafety, "{titles:?}");
    assert_eq!(tasks[3].priority, PriorityClass::PropertyLoss, "{titles:?}");
    // Four different things, and no once-a-decade replacement.
    let mut items: Vec<_> = tasks.iter().map(|t| t.item).collect();
    items.dedup();
    assert_eq!(items.len(), 4);
    assert!(
        tasks.iter().all(|t| t.schedule.period_days() <= 366),
        "{titles:?}"
    );
    assert_eq!(active(&h.store).len(), 4);

    // Children at home: five.
    let mut q = philly_house(h.property);
    q.people.children = Some(1);
    assert_eq!(starting_count(&q), 5);
}

#[test]
fn redoing_the_questions_only_adds() {
    let mut h = home();
    let mut p = philly_house(h.property);
    let l = lib();
    h.store
        .apply_profile(&p, &l, h.owner, date(2026, 11, 27))
        .unwrap();
    h.store
        .switch_on_starting_plan(h.property, 4, h.now())
        .unwrap();
    let items_before: Vec<Item> = h.store.list().unwrap();
    let tasks_before: Vec<Task> = h.store.list().unwrap();

    // Same answers again: nothing new, nothing changed.
    let again = h
        .store
        .apply_profile(&p, &l, h.owner, date(2026, 12, 1))
        .unwrap();
    assert!(again.new_items.is_empty() && again.new_tasks.is_empty());
    assert_eq!(h.store.list::<Task>().unwrap(), tasks_before);

    // New answers: a pool and a well. Old things and switched-on tasks stay;
    // a "no" to something recorded earlier does not delete it.
    p.features.insert(Feature::Pool, true);
    p.features.insert(Feature::Dishwasher, false);
    p.water_supply = Some(WaterSupply::Well);
    let more = h
        .store
        .apply_profile(&p, &l, h.owner, date(2026, 12, 1))
        .unwrap();
    assert_eq!(more.new_items.len(), 2);
    let items_after: Vec<Item> = h.store.list().unwrap();
    assert_eq!(items_after.len(), items_before.len() + 2);
    for i in &items_before {
        assert!(items_after.contains(i));
    }
    let tasks_after: Vec<Task> = h.store.list().unwrap();
    for t in &tasks_before {
        assert!(tasks_after.contains(t), "{} changed", t.title);
    }
    assert_eq!(active(&h.store).len(), 4, "redo switches nothing on");
    assert!(
        more.new_tasks
            .iter()
            .all(|id| !h.store.fetch::<Task>(&id.to_string()).unwrap().active)
    );
}

#[test]
fn weekly_suggestion_add_not_for_us_and_later() {
    let mut h = home();
    // A Philadelphia house in mid October, alarms recorded.
    h.clock.set(NOW_MS - 46 * DAY_MS); // 2026-10-12
    let mut p = Profile::empty(h.property);
    p.home_type = Some(HomeType::House);
    p.zip = Some(Flagged::normal("19103".into()));
    p.smoke_alarms = Some(AlarmCount::One);
    p.co_alarms = Some(AlarmCount::One);
    for f in Feature::ALL {
        p.features.insert(f, false);
    }
    let l = lib();
    h.store
        .apply_profile(&p, &l, h.owner, date(2026, 10, 12))
        .unwrap();
    h.store
        .switch_on_starting_plan(h.property, 4, h.now())
        .unwrap();
    let s = h
        .store
        .weekly_suggestion(h.property, h.now())
        .unwrap()
        .expect("a suggestion");
    assert_eq!(s.key, "ask:hose-bibs", "{s:?}");

    // Add: the thing appears with its winterising task switched on.
    let switched = h
        .store
        .answer_suggestion(h.property, &s.key, Answer::Added, &l, h.owner, h.now())
        .unwrap();
    assert_eq!(switched.len(), 1);
    let t: Task = h.store.fetch(&switched[0].to_string()).unwrap();
    assert_eq!(t.title, "Winterise the outdoor hose taps");
    assert!(t.active);
    // One a week: nothing more this week.
    assert!(
        h.store
            .weekly_suggestion(h.property, h.now())
            .unwrap()
            .is_none()
    );

    // Next week: something else. Say "Not for us".
    h.clock.advance(8 * DAY_MS);
    let s2 = h
        .store
        .weekly_suggestion(h.property, h.now())
        .unwrap()
        .unwrap();
    assert_ne!(s2.key, s.key);
    h.store
        .answer_suggestion(h.property, &s2.key, Answer::NotForUs, &l, h.owner, h.now())
        .unwrap();
    // It never comes back, even a year later.
    for weeks in [1, 5, 52] {
        h.clock
            .set(NOW_MS - 38 * DAY_MS + weeks * 7 * DAY_MS + DAY_MS);
        let (all, _) = h.store.suggestions(h.property, h.now()).unwrap();
        assert!(all.iter().all(|x| x.key != s2.key), "after {weeks} weeks");
    }

    // Later: hidden for four weeks, then back.
    let (all, _) = h.store.suggestions(h.property, h.now()).unwrap();
    let s3 = all.first().expect("more suggestions").clone();
    h.store
        .answer_suggestion(h.property, &s3.key, Answer::Later, &l, h.owner, h.now())
        .unwrap();
    let (soon, answered) = h.store.suggestions(h.property, h.now()).unwrap();
    assert!(answered);
    assert!(soon.iter().all(|x| x.key != s3.key));
    // Answers are append-only records.
    assert!(
        h.store
            .delete(
                EntityKind::SuggestionAnswer,
                &h.store.suggestion_answers(h.property).unwrap()[0]
                    .id
                    .to_string()
            )
            .is_err()
    );
    // Unknown keys are refused.
    assert!(
        h.store
            .answer_suggestion(
                h.property,
                "ask:nothing",
                Answer::Added,
                &l,
                h.owner,
                h.now()
            )
            .is_err()
    );
}

#[test]
fn profile_and_answers_survive_export_and_import() {
    let mut h = home();
    let p = philly_house(h.property);
    let l = lib();
    h.store
        .apply_profile(&p, &l, h.owner, date(2026, 11, 27))
        .unwrap();
    let s = h
        .store
        .weekly_suggestion(h.property, h.now())
        .unwrap()
        .unwrap();
    h.store
        .answer_suggestion(h.property, &s.key, Answer::Later, &l, h.owner, h.now())
        .unwrap();
    let e = export::export(&h.store).unwrap();
    assert_eq!(e.version, export::EXPORT_VERSION);
    assert!(e.entities.contains_key(&EntityKind::Profile));
    assert!(e.entities.contains_key(&EntityKind::SuggestionAnswer));
    let text = export::to_json(&e).unwrap();
    let mut copy = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let r = export::import(&mut copy, &export::from_json(&text).unwrap(), None).unwrap();
    assert!(r.with_history);
    assert_eq!(copy.profile(h.property).unwrap().unwrap(), p);
    assert_eq!(copy.suggestion_answers(h.property).unwrap().len(), 1);
}

#[test]
fn a_phase_2_export_still_imports() {
    let text = include_str!("fixtures/export-v1-phase2.json");
    let e = export::from_json(text).unwrap();
    assert_eq!(e.version, 1);
    let mut s = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let r = export::import(&mut s, &e, None).unwrap();
    assert!(r.with_history);
    let props: Vec<Property> = s.list().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].climate.hard_freeze, None);
    assert!(s.profile(props[0].id).unwrap().is_none());
    // And the questions can be answered afterwards.
    let owner = s.author().unwrap();
    let p = philly_house(props[0].id);
    let out = s
        .apply_profile(&p, &lib(), owner, date(2026, 11, 27))
        .unwrap();
    // The smoke alarm from Phase 2 is not added twice.
    let smoke = s
        .list::<Item>()
        .unwrap()
        .into_iter()
        .filter(|i| i.template.as_deref() == Some("smoke-alarm"))
        .count();
    assert_eq!(smoke, 1);
    assert!(!out.new_items.is_empty());
}
