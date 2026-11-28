//! The home profile: answers to the onboarding questions (D-24, D-65), and
//! the pure mapping from answers to the things a house probably has.
//!
//! Every answer is optional: a missing answer means "Not sure" and creates
//! nothing. The profile is an ordinary last-writer-wins entity, one per
//! property (its id is the property's id), so it goes through the operation
//! log and merges field by field like everything else.
//!
//! Only counts are kept about the people who live there, never names or
//! ages. The ZIP code is a location, so it carries a sensitivity flag; it is
//! marked normal (a ZIP code alone is not a secret), but the flag means the
//! binder can file it elsewhere if that ever changes (D-41).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::PropertyId;
use crate::model::Flagged;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeType {
    House,
    Townhouse,
    Condo,
    Apartment,
    Mobile,
}

impl HomeType {
    /// Has its own roof, walls and outside to look after.
    pub fn has_exterior(self) -> bool {
        matches!(
            self,
            HomeType::House | HomeType::Townhouse | HomeType::Mobile
        )
    }
}

/// Roughly when it was built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltEra {
    Before1950,
    From1950To1979,
    From1980To1999,
    From2000To2019,
    From2020,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Heating {
    GasFurnace,
    Oil,
    HeatPump,
    /// Electric baseboard or other resistance heat: nothing to service.
    Electric,
    Boiler,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cooling {
    CentralAc,
    HeatPump,
    WindowUnits,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterHeaterKind {
    TankGas,
    TankElectric,
    Tankless,
    HeatPump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterSupply {
    City,
    Well,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sewer {
    City,
    Septic,
}

/// Things a home may or may not have (the presence checklist).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Basement,
    SumpPump,
    Crawlspace,
    Attic,
    Fireplace,
    WoodStove,
    Garage,
    Gutters,
    Yard,
    Pool,
    Deck,
    Dryer,
    Dishwasher,
    Washer,
    FridgeWater,
    Disposal,
    RangeHood,
    Dehumidifier,
    WaterSoftener,
    Generator,
    Solar,
}

impl Feature {
    pub const ALL: [Feature; 21] = [
        Feature::Basement,
        Feature::SumpPump,
        Feature::Crawlspace,
        Feature::Attic,
        Feature::Fireplace,
        Feature::WoodStove,
        Feature::Garage,
        Feature::Gutters,
        Feature::Yard,
        Feature::Pool,
        Feature::Deck,
        Feature::Dryer,
        Feature::Dishwasher,
        Feature::Washer,
        Feature::FridgeWater,
        Feature::Disposal,
        Feature::RangeHood,
        Feature::Dehumidifier,
        Feature::WaterSoftener,
        Feature::Generator,
        Feature::Solar,
    ];

    /// The template a "yes" creates, the item's name, and its room.
    pub fn thing(self) -> Option<(&'static str, &'static str, Option<&'static str>)> {
        Some(match self {
            Feature::Basement | Feature::Garage => return None,
            Feature::SumpPump => ("sump-pump", "Sump pump", Some(BASEMENT)),
            Feature::Crawlspace => ("crawlspace", "Crawlspace", None),
            Feature::Attic => ("attic", "Attic", None),
            Feature::Fireplace => ("fireplace", "Fireplace and chimney", Some("Living room")),
            Feature::WoodStove => ("wood-stove", "Wood stove", None),
            Feature::Gutters => ("gutters", "Gutters and downspouts", None),
            Feature::Yard => ("lawn-mower", "Lawn mower", None),
            Feature::Pool => ("pool", "Swimming pool", None),
            Feature::Deck => ("deck", "Deck", None),
            Feature::Dryer => ("clothes-dryer", "Clothes dryer", Some(LAUNDRY)),
            Feature::Dishwasher => ("dishwasher", "Dishwasher", Some(KITCHEN)),
            Feature::Washer => ("washing-machine", "Washing machine", Some(LAUNDRY)),
            Feature::FridgeWater => ("refrigerator", "Refrigerator", Some(KITCHEN)),
            Feature::Disposal => ("garbage-disposal", "Garbage disposal", Some(KITCHEN)),
            Feature::RangeHood => ("range-hood", "Range hood", Some(KITCHEN)),
            Feature::Dehumidifier => ("dehumidifier", "Dehumidifier", Some(BASEMENT)),
            Feature::WaterSoftener => ("water-softener", "Water softener", None),
            Feature::Generator => ("generator", "Generator", None),
            Feature::Solar => ("solar-panels", "Solar panels", None),
        })
    }
}

const BASEMENT: &str = "Basement";
const KITCHEN: &str = "Kitchen";
const LAUNDRY: &str = "Laundry";

/// Roughly how many smoke or CO alarms there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmCount {
    None,
    One,
    TwoToThree,
    FourOrMore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmPower {
    Battery,
    Hardwired,
    Sealed,
}

/// Who lives there, as counts only. Used to tune priorities (D-66).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct People {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adults: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older_adults: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pets: Option<bool>,
}

impl People {
    /// Someone who may need more time or help to get out in an emergency.
    pub fn has_vulnerable(&self) -> bool {
        self.children.unwrap_or(0) > 0 || self.older_adults.unwrap_or(0) > 0
    }
}

/// The answers. `None` everywhere means "Not sure" (or not asked yet).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The property's id: one profile per property.
    pub id: PropertyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_type: Option<HomeType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built: Option<BuiltEra>,
    /// US ZIP code, used only to look up the climate on the device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zip: Option<Flagged<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heating: Option<Vec<Heating>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooling: Option<Vec<Cooling>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_heater: Option<WaterHeaterKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_supply: Option<WaterSupply>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sewer: Option<Sewer>,
    /// The presence checklist: true = has it, false = does not; absent = not sure.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<Feature, bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smoke_alarms: Option<AlarmCount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smoke_power: Option<AlarmPower>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub co_alarms: Option<AlarmCount>,
    #[serde(default)]
    pub people: People,
}
crate::entity!(Profile, Profile);

impl Profile {
    pub fn empty(property: PropertyId) -> Self {
        Self {
            id: property,
            home_type: None,
            built: None,
            zip: None,
            heating: None,
            cooling: None,
            water_heater: None,
            water_supply: None,
            sewer: None,
            features: BTreeMap::new(),
            smoke_alarms: None,
            smoke_power: None,
            co_alarms: None,
            people: People::default(),
        }
    }

    pub fn has(&self, f: Feature) -> Option<bool> {
        self.features.get(&f).copied()
    }

    fn heats_with(&self, h: Heating) -> bool {
        self.heating.as_ref().is_some_and(|v| v.contains(&h))
    }

    fn cools_with(&self, c: Cooling) -> bool {
        self.cooling.as_ref().is_some_and(|v| v.contains(&c))
    }

    /// Something in the house burns fuel, so carbon monoxide is possible.
    pub fn burns_fuel(&self) -> bool {
        self.heats_with(Heating::GasFurnace)
            || self.heats_with(Heating::Oil)
            || self.heats_with(Heating::Boiler)
            || self.water_heater == Some(WaterHeaterKind::TankGas)
            || self.has(Feature::Fireplace) == Some(true)
            || self.has(Feature::WoodStove) == Some(true)
            || self.has(Feature::Garage) == Some(true)
            || self.has(Feature::Generator) == Some(true)
    }

    /// How many answers have been given (for "N of 10 answered").
    pub fn answered(&self) -> usize {
        [
            self.home_type.is_some(),
            self.built.is_some(),
            self.zip.is_some(),
            self.heating.is_some(),
            self.cooling.is_some(),
            self.water_heater.is_some(),
            self.water_supply.is_some() || self.sewer.is_some(),
            !self.features.is_empty(),
            self.smoke_alarms.is_some() || self.co_alarms.is_some(),
            self.people != People::default(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }
}

/// One thing the answers say the house has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedThing {
    pub template: String,
    pub name: String,
    /// A room where it obviously goes ("Kitchen"); otherwise unassigned.
    pub room: Option<String>,
    /// Field values, as text ("gas", "yes").
    pub values: BTreeMap<String, String>,
}

fn thing(template: &str, name: &str, room: Option<&str>, values: &[(&str, &str)]) -> PlannedThing {
    PlannedThing {
        template: template.into(),
        name: name.into(),
        room: room.map(str::to_string),
        values: values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// The things a house with these answers has, one per template, in a stable
/// order. Pure. "Not sure" answers add nothing; just-in-time questions
/// (`crate::suggest`) come back to them later.
pub fn plan_home(p: &Profile) -> Vec<PlannedThing> {
    let basement = (p.has(Feature::Basement) == Some(true)).then_some(BASEMENT);
    let mut out: Vec<PlannedThing> = Vec::new();
    let mut push = |t: PlannedThing| {
        if !out.iter().any(|o| o.template == t.template) {
            out.push(t);
        }
    };

    // Life safety first, so the starting plan and "things recorded" lead with them.
    if matches!(
        p.smoke_alarms,
        Some(AlarmCount::One | AlarmCount::TwoToThree | AlarmCount::FourOrMore)
    ) {
        let power = match p.smoke_power {
            Some(AlarmPower::Battery) => Some("battery"),
            Some(AlarmPower::Hardwired) => Some("hardwired_battery_backup"),
            Some(AlarmPower::Sealed) => Some("sealed_10_year"),
            None => None,
        };
        let name = if p.smoke_alarms == Some(AlarmCount::One) {
            "Smoke alarm"
        } else {
            "Smoke alarms"
        };
        let values: Vec<(&str, &str)> = power.map(|v| ("power", v)).into_iter().collect();
        push(thing("smoke-alarm", name, None, &values));
    }
    if matches!(
        p.co_alarms,
        Some(AlarmCount::One | AlarmCount::TwoToThree | AlarmCount::FourOrMore)
    ) {
        let name = if p.co_alarms == Some(AlarmCount::One) {
            "CO alarm"
        } else {
            "CO alarms"
        };
        push(thing("co-alarm", name, None, &[]));
    }

    // Heating and cooling.
    if p.heats_with(Heating::GasFurnace) {
        push(thing("furnace", "Furnace", basement, &[("fuel", "gas")]));
    }
    if p.heats_with(Heating::Oil) {
        push(thing("furnace", "Furnace", basement, &[("fuel", "oil")]));
    }
    if p.heats_with(Heating::Boiler) {
        let fuel: &[(&str, &str)] = if p.heats_with(Heating::Oil) {
            &[("fuel", "oil")]
        } else if p.heats_with(Heating::GasFurnace)
            || p.water_heater == Some(WaterHeaterKind::TankGas)
        {
            &[("fuel", "gas")]
        } else {
            &[]
        };
        push(thing("boiler", "Boiler", basement, fuel));
    }
    if p.heats_with(Heating::HeatPump) || p.cools_with(Cooling::HeatPump) {
        push(thing("heat-pump", "Heat pump", None, &[]));
    }
    if p.cools_with(Cooling::CentralAc) {
        push(thing("central-ac", "Central air conditioner", None, &[]));
    }
    let ducted = p.heats_with(Heating::GasFurnace)
        || p.heats_with(Heating::Oil)
        || p.heats_with(Heating::HeatPump)
        || p.cools_with(Cooling::HeatPump)
        || p.cools_with(Cooling::CentralAc);
    if ducted {
        push(thing(
            "hvac-air-filter",
            "Furnace / HVAC air filter",
            basement,
            &[],
        ));
    }
    if p.cools_with(Cooling::WindowUnits) {
        push(thing("window-ac", "Window air conditioners", None, &[]));
    }

    // Water.
    match p.water_heater {
        Some(WaterHeaterKind::TankGas) => push(thing(
            "water-heater",
            "Water heater",
            basement,
            &[("fuel", "gas")],
        )),
        Some(WaterHeaterKind::TankElectric) => push(thing(
            "water-heater",
            "Water heater",
            basement,
            &[("fuel", "electric")],
        )),
        Some(WaterHeaterKind::Tankless) => push(thing(
            "tankless-water-heater",
            "Tankless water heater",
            None,
            &[],
        )),
        Some(WaterHeaterKind::HeatPump) => push(thing(
            "heat-pump-water-heater",
            "Heat pump water heater",
            basement,
            &[],
        )),
        None => {}
    }
    push(thing(
        "main-water-shutoff",
        "Main water shutoff",
        basement,
        &[],
    ));
    if p.water_supply == Some(WaterSupply::Well) {
        push(thing("well-pump", "Well and pressure tank", None, &[]));
    }
    if p.sewer == Some(Sewer::Septic) {
        push(thing("septic-system", "Septic system", None, &[]));
    }

    // The checklist.
    for f in Feature::ALL {
        if p.has(f) == Some(true)
            && let Some((template, name, room)) = f.thing()
        {
            let room = if room == Some(BASEMENT) {
                basement
            } else {
                room
            };
            let values: &[(&str, &str)] = if f == Feature::FridgeWater {
                &[("water_filter", "yes")]
            } else {
                &[]
            };
            push(thing(template, name, room, values));
        }
    }

    // Every home.
    push(thing(
        "electrical-panel",
        "Electrical panel, GFCI and AFCI",
        basement,
        &[],
    ));
    push(thing("toilets", "Toilets", None, &[]));
    push(thing("under-sink-plumbing", "Pipes under sinks", None, &[]));
    push(thing(
        "tub-shower-caulk",
        "Tub and shower caulk",
        Some("Bathroom"),
        &[],
    ));
    push(thing("windows-doors", "Windows and doors", None, &[]));
    if p.home_type.is_some_and(HomeType::has_exterior) {
        push(thing("roof", "Roof", None, &[]));
        push(thing(
            "exterior-siding",
            "Siding and exterior paint",
            None,
            &[],
        ));
    }
    out
}

/// How many tasks to switch on after onboarding (D-66): four, or five when
/// children or older adults live there, so the extra place goes to the next
/// life-safety task.
pub fn starting_count(p: &Profile) -> usize {
    if p.people.has_vulnerable() { 5 } else { 4 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn templates(p: &Profile) -> Vec<String> {
        plan_home(p).into_iter().map(|t| t.template).collect()
    }

    #[test]
    fn nothing_answered_gives_only_what_every_home_has() {
        let p = Profile::empty(PropertyId::new());
        assert_eq!(
            templates(&p),
            [
                "main-water-shutoff",
                "electrical-panel",
                "toilets",
                "under-sink-plumbing",
                "tub-shower-caulk",
                "windows-doors"
            ]
        );
        assert_eq!(p.answered(), 0);
        assert!(!p.burns_fuel());
    }

    #[test]
    fn a_full_house() {
        let mut p = Profile::empty(PropertyId::new());
        p.home_type = Some(HomeType::House);
        p.heating = Some(vec![Heating::GasFurnace]);
        p.cooling = Some(vec![Cooling::CentralAc]);
        p.water_heater = Some(WaterHeaterKind::TankGas);
        p.water_supply = Some(WaterSupply::Well);
        p.sewer = Some(Sewer::Septic);
        p.smoke_alarms = Some(AlarmCount::TwoToThree);
        p.smoke_power = Some(AlarmPower::Battery);
        p.co_alarms = Some(AlarmCount::One);
        for f in [
            Feature::Basement,
            Feature::SumpPump,
            Feature::FridgeWater,
            Feature::Washer,
        ] {
            p.features.insert(f, true);
        }
        p.features.insert(Feature::Pool, false);
        let plan = plan_home(&p);
        let t: Vec<&str> = plan.iter().map(|t| t.template.as_str()).collect();
        assert_eq!(&t[..2], ["smoke-alarm", "co-alarm"]);
        for want in [
            "furnace",
            "central-ac",
            "hvac-air-filter",
            "water-heater",
            "well-pump",
            "septic-system",
            "sump-pump",
            "refrigerator",
            "washing-machine",
            "roof",
        ] {
            assert!(t.contains(&want), "missing {want}");
        }
        assert!(!t.contains(&"pool"));
        let smoke = &plan[0];
        assert_eq!(smoke.name, "Smoke alarms");
        assert_eq!(smoke.values["power"], "battery");
        let furnace = plan.iter().find(|t| t.template == "furnace").unwrap();
        assert_eq!(furnace.values["fuel"], "gas");
        assert_eq!(furnace.room.as_deref(), Some("Basement"));
        let fridge = plan.iter().find(|t| t.template == "refrigerator").unwrap();
        assert_eq!(fridge.values["water_filter"], "yes");
        assert_eq!(fridge.room.as_deref(), Some("Kitchen"));
        // One template, one thing.
        let mut uniq = t.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), t.len());
        assert!(p.burns_fuel());
        assert_eq!(starting_count(&p), 4);
    }

    #[test]
    fn apartment_has_no_roof_and_no_alarm_means_no_alarm_item() {
        let mut p = Profile::empty(PropertyId::new());
        p.home_type = Some(HomeType::Apartment);
        p.smoke_alarms = Some(AlarmCount::None);
        p.people.children = Some(2);
        let t = templates(&p);
        assert!(!t.contains(&"roof".to_string()));
        assert!(!t.contains(&"smoke-alarm".to_string()));
        assert_eq!(starting_count(&p), 5);
    }

    #[test]
    fn profile_json_is_compact() {
        let mut p = Profile::empty(PropertyId::new());
        p.features.insert(Feature::SumpPump, true);
        let v = serde_json::to_value(&p).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["features", "id", "people"]);
        assert_eq!(v["features"]["sump_pump"], true);
        let back: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }
}
