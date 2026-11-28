//! Plain wording for dates, money, schedules and the home profile.

use housewright_core::fields::FieldValue;
use housewright_core::profile::*;
use housewright_core::schedule::{Schedule, Season, Unit};
use jiff::civil::Date;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "Nov 27, 2026"
pub fn date(d: Date) -> String {
    format!(
        "{} {}, {}",
        MONTHS[(d.month() - 1) as usize],
        d.day(),
        d.year()
    )
}

/// "November 2026"
pub fn month(d: Date) -> String {
    format!("{} {}", MONTHS_LONG[(d.month() - 1) as usize], d.year())
}

fn month_name(m: u8) -> &'static str {
    MONTHS_LONG
        .get(usize::from(m.clamp(1, 12)) - 1)
        .copied()
        .unwrap_or("January")
}

/// "about 7 years old"
pub fn age(from: Date, today: Date) -> Option<String> {
    let months = (i32::from(today.year()) - i32::from(from.year())) * 12 + i32::from(today.month())
        - i32::from(from.month());
    if months < 0 {
        return None;
    }
    Some(if months < 12 {
        "less than a year old".into()
    } else if months < 24 {
        "about a year old".into()
    } else {
        format!("about {} years old", months / 12)
    })
}

/// Minor units in a currency: "$1,299.99", "€12.50", "12.50 CHF".
pub fn money(minor: i64, currency: &str) -> String {
    let neg = minor < 0;
    let m = minor.unsigned_abs();
    let whole = (m / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let n = format!("{grouped}.{:02}", m % 100);
    let sym = match currency {
        "USD" | "CAD" | "AUD" | "NZD" => Some("$"),
        "EUR" => Some("€"),
        "GBP" => Some("£"),
        _ => None,
    };
    let s = match sym {
        Some(s) => format!("{s}{n}"),
        None => format!("{n} {currency}"),
    };
    if neg { format!("-{s}") } else { s }
}

fn unit(n: u32, u: Unit) -> String {
    let w = match u {
        Unit::Day => "day",
        Unit::Week => "week",
        Unit::Month => "month",
        Unit::Year => "year",
    };
    if n == 1 {
        format!("1 {w}")
    } else {
        format!("{n} {w}s")
    }
}

fn every(n: u32, u: Unit) -> String {
    if n == 1 {
        match u {
            Unit::Day => "Every day".into(),
            Unit::Week => "Every week".into(),
            Unit::Month => "Every month".into(),
            Unit::Year => "Every year".into(),
        }
    } else {
        format!("Every {}", unit(n, u))
    }
}

fn months_list(ms: &[u8]) -> String {
    let names: Vec<&str> = ms.iter().map(|m| month_name(*m)).collect();
    match names.len() {
        0 => "every month".into(),
        1 => names[0].into(),
        _ => format!(
            "{} and {}",
            names[..names.len() - 1].join(", "),
            names[names.len() - 1]
        ),
    }
}

fn season(s: Season) -> &'static str {
    match s {
        Season::Spring => "Every spring",
        Season::Summer => "Every summer",
        Season::Autumn => "Every autumn",
        Season::Winter => "Every winter",
        Season::BeforeFirstFrost => "Every year before the first frost",
        Season::AfterLastFrost => "Every spring after the last frost",
    }
}

/// How often, in words: "Every 3 months", "In March and September".
pub fn schedule(s: &Schedule) -> String {
    match s {
        Schedule::Fixed { every: e, .. } => every(e.n, e.unit),
        Schedule::Floating { every: e } => format!("{} after it was last done", unit(e.n, e.unit)),
        Schedule::NthWeekday {
            nth,
            weekday,
            months,
        } => {
            let which = match nth {
                -1 => "last".to_string(),
                1 => "first".into(),
                2 => "second".into(),
                3 => "third".into(),
                n => format!("{n}th"),
            };
            let day = format!("{weekday:?}");
            if months.is_empty() {
                format!("The {which} {day} of every month")
            } else {
                format!("The {which} {day} of {}", months_list(months))
            }
        }
        Schedule::MonthsOfYear { months, day } => {
            if *day <= 1 {
                format!("Every {}", months_list(months))
            } else {
                format!("Every {} (around day {day})", months_list(months))
            }
        }
        Schedule::Seasonal { season: s } => season(*s).into(),
        Schedule::Usage {
            meter,
            threshold,
            or_every,
        } => {
            let m = meter.replace('_', " ");
            match or_every {
                Some(i) => format!(
                    "Every {threshold} {m}, or {}, whichever comes first",
                    unit(i.n, i.unit)
                ),
                None => format!("Every {threshold} {m}"),
            }
        }
        Schedule::OneOff { due } => format!("Once, on {}", date(*due)),
        Schedule::Triggered { event } => {
            format!("When a {} alert comes in", event.replace('_', " "))
        }
    }
}

/// A custom field value as text, or `None` for photos (printed as photos).
pub fn field_value(v: &FieldValue) -> Option<String> {
    Some(match v {
        FieldValue::Text(s) | FieldValue::Choice(s) => s.clone(),
        FieldValue::Number { value, unit } => {
            let n = if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                format!("{value}")
            };
            match unit {
                Some(u) => format!("{n} {u}"),
                None => n,
            }
        }
        FieldValue::Date(d) => date(*d),
        FieldValue::YesNo(b) => if *b { "Yes" } else { "No" }.into(),
        FieldValue::Photo(_) => return None,
    })
}

fn join<T>(v: &[T], f: impl Fn(&T) -> &'static str) -> String {
    v.iter().map(f).collect::<Vec<_>>().join(", ")
}

/// The home questions' answers as label and value pairs.
pub fn profile(p: &Profile) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut put = |k: &str, v: String| {
        if !v.is_empty() {
            out.push((k.to_string(), v));
        }
    };
    if let Some(t) = p.home_type {
        put(
            "Kind of home",
            match t {
                HomeType::House => "House",
                HomeType::Townhouse => "Townhouse",
                HomeType::Condo => "Condo",
                HomeType::Apartment => "Apartment",
                HomeType::Mobile => "Mobile or manufactured home",
            }
            .into(),
        );
    }
    if let Some(b) = p.built {
        put(
            "Built",
            match b {
                BuiltEra::Before1950 => "Before 1950",
                BuiltEra::From1950To1979 => "1950 to 1979",
                BuiltEra::From1980To1999 => "1980 to 1999",
                BuiltEra::From2000To2019 => "2000 to 2019",
                BuiltEra::From2020 => "2020 or later",
            }
            .into(),
        );
    }
    if let Some(h) = &p.heating {
        put(
            "Heating",
            join(h, |x| match x {
                Heating::GasFurnace => "Gas furnace",
                Heating::Oil => "Oil",
                Heating::HeatPump => "Heat pump",
                Heating::Electric => "Electric heaters",
                Heating::Boiler => "Boiler",
                Heating::Other => "Other",
            }),
        );
    }
    if let Some(c) = &p.cooling {
        put(
            "Cooling",
            join(c, |x| match x {
                Cooling::CentralAc => "Central air conditioning",
                Cooling::HeatPump => "Heat pump",
                Cooling::WindowUnits => "Window units",
                Cooling::None => "None",
            }),
        );
    }
    if let Some(w) = p.water_heater {
        put(
            "Water heater",
            match w {
                WaterHeaterKind::TankGas => "Gas tank",
                WaterHeaterKind::TankElectric => "Electric tank",
                WaterHeaterKind::Tankless => "Tankless",
                WaterHeaterKind::HeatPump => "Heat pump",
            }
            .into(),
        );
    }
    if let Some(w) = p.water_supply {
        put(
            "Water supply",
            match w {
                WaterSupply::City => "City water",
                WaterSupply::Well => "Well",
            }
            .into(),
        );
    }
    if let Some(s) = p.sewer {
        put(
            "Waste water",
            match s {
                Sewer::City => "City sewer",
                Sewer::Septic => "Septic system",
            }
            .into(),
        );
    }
    let has: Vec<&'static str> = p
        .features
        .iter()
        .filter(|(_, v)| **v)
        .map(|(f, _)| feature(*f))
        .collect();
    if !has.is_empty() {
        put("Also has", has.join(", "));
    }
    let count = |c: AlarmCount| match c {
        AlarmCount::None => "None",
        AlarmCount::One => "One",
        AlarmCount::TwoToThree => "Two or three",
        AlarmCount::FourOrMore => "Four or more",
    };
    if let Some(c) = p.smoke_alarms {
        let power = p.smoke_power.map(|x| match x {
            AlarmPower::Battery => " (battery)",
            AlarmPower::Hardwired => " (wired in)",
            AlarmPower::Sealed => " (sealed 10-year battery)",
        });
        put(
            "Smoke alarms",
            format!("{}{}", count(c), power.unwrap_or("")),
        );
    }
    if let Some(c) = p.co_alarms {
        put("Carbon monoxide alarms", count(c).into());
    }
    out
}

fn feature(f: Feature) -> &'static str {
    match f {
        Feature::Basement => "basement",
        Feature::SumpPump => "sump pump",
        Feature::Crawlspace => "crawlspace",
        Feature::Attic => "attic",
        Feature::Fireplace => "fireplace",
        Feature::WoodStove => "wood stove",
        Feature::Garage => "garage",
        Feature::Gutters => "gutters",
        Feature::Yard => "yard",
        Feature::Pool => "pool",
        Feature::Deck => "deck",
        Feature::Dryer => "clothes dryer",
        Feature::Dishwasher => "dishwasher",
        Feature::Washer => "washing machine",
        Feature::FridgeWater => "fridge with water line",
        Feature::Disposal => "garbage disposal",
        Feature::RangeHood => "range hood",
        Feature::Dehumidifier => "dehumidifier",
        Feature::WaterSoftener => "water softener",
        Feature::Generator => "generator",
        Feature::Solar => "solar panels",
    }
}

/// Footer name and title for a template category.
pub fn category(key: Option<&str>) -> (&'static str, &'static str) {
    match key.unwrap_or("general") {
        "safety" => ("Safety", "Safety"),
        "heating_cooling" => ("Heating", "Heating and cooling"),
        "plumbing" => ("Water", "Plumbing and water"),
        "appliances" => ("Appliances", "Appliances"),
        "electrical" => ("Electrical", "Electrical and power"),
        "exterior" => ("Outside", "Outside the house"),
        "structure" => ("Structure", "Roof, attic and structure"),
        "yard" => ("Yard", "Yard, garden and pool"),
        _ => ("Other", "Other things"),
    }
}

/// The order categories are printed in: safety first.
pub const CATEGORY_ORDER: [&str; 9] = [
    "safety",
    "plumbing",
    "heating_cooling",
    "electrical",
    "appliances",
    "structure",
    "exterior",
    "yard",
    "general",
];

#[cfg(test)]
mod tests {
    use super::*;
    use housewright_core::schedule::Interval;

    #[test]
    fn wording() {
        assert_eq!(money(129999, "USD"), "$1,299.99");
        assert_eq!(money(5, "EUR"), "€0.05");
        assert_eq!(money(1250, "CHF"), "12.50 CHF");
        assert_eq!(date(jiff::civil::date(2026, 11, 27)), "Nov 27, 2026");
        assert_eq!(
            schedule(&Schedule::Fixed {
                every: Interval::months(3),
                anchor: None
            }),
            "Every 3 months"
        );
        assert_eq!(
            schedule(&Schedule::MonthsOfYear {
                months: vec![3, 9],
                day: 1
            }),
            "Every March and September"
        );
        assert_eq!(
            age(
                jiff::civil::date(2019, 5, 1),
                jiff::civil::date(2026, 11, 27)
            )
            .unwrap(),
            "about 7 years old"
        );
    }
}
