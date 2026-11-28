//! The home facts sheet (D-27): quick-reference facts per property, in
//! fixed groups, each marked normal or sensitive (D-41) so the Phase 5
//! binder can file it into the main or the sealed volume.
//!
//! This module is pure: the groups, the default sensitivity of a new fact,
//! and the questions worth asking given what the home profile says.

use serde::Serialize;

use crate::model::{Fact, Item, Sensitivity};
use crate::profile::{Heating, Profile, Sewer, WaterHeaterKind, WaterSupply};

/// The groups, in the order the sheet shows them: (key, label, sensitive by
/// default).
pub const GROUPS: &[(&str, &str, bool)] = &[
    ("shutoffs", "Shutoffs and safety", false),
    ("sizes", "Sizes and supplies", false),
    ("finishes", "Finishes", false),
    ("access", "Access", true),
    ("utilities", "Utilities and accounts", true),
    ("other", "Other", false),
];

pub fn group_label(key: &str) -> &'static str {
    GROUPS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, l, _)| *l)
        .unwrap_or("Other")
}

pub fn is_group(key: &str) -> bool {
    GROUPS.iter().any(|(k, _, _)| *k == key)
}

/// Words in a label that make a fact sensitive whatever its group: codes,
/// passwords, combinations, account and policy numbers, where valuables are.
const SENSITIVE_WORDS: &[&str] = &[
    "code",
    "password",
    "passcode",
    "pin",
    "combination",
    "account",
    "policy",
    "safe",
    "spare key",
    "hidden key",
    "key is",
    "wi-fi",
    "wifi",
    "lockbox",
    "valuables",
    "jewellery",
    "jewelry",
];

/// The sensitivity a new fact gets unless the person chooses otherwise.
/// Errs on the side of sensitive: a value wrongly hidden costs a tap; a
/// value wrongly printed in the main binder cannot be unprinted.
pub fn default_sensitivity(group: &str, label: &str) -> Sensitivity {
    let l = label.to_lowercase();
    let by_group = GROUPS.iter().any(|(k, _, s)| *k == group && *s);
    let by_word = SENSITIVE_WORDS.iter().any(|w| {
        // "pin" must be a word, not part of "spinning" or "piping".
        if *w == "pin" {
            l.split(|c: char| !c.is_alphanumeric()).any(|x| x == "pin")
        } else {
            l.contains(w)
        }
    });
    if by_group || by_word {
        Sensitivity::Sensitive
    } else {
        Sensitivity::Normal
    }
}

/// A question the facts sheet suggests answering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FactPrompt {
    /// Stored on the fact that answers it, so it is not asked again.
    pub key: &'static str,
    pub group: &'static str,
    /// The fact's label once answered.
    pub label: &'static str,
    pub question: &'static str,
    /// Placeholder showing the kind of answer wanted.
    pub hint: &'static str,
    pub sensitive: bool,
}

const fn prompt(
    key: &'static str,
    group: &'static str,
    label: &'static str,
    question: &'static str,
    hint: &'static str,
    sensitive: bool,
) -> FactPrompt {
    FactPrompt {
        key,
        group,
        label,
        question,
        hint,
        sensitive,
    }
}

/// Questions not yet answered, most important first. Uses the home
/// profile when there is one; without it, asks the questions that apply to
/// nearly every home and skips the rest.
pub fn prompts(profile: Option<&Profile>, items: &[Item], facts: &[Fact]) -> Vec<FactPrompt> {
    let answered = |k: &str| facts.iter().any(|f| f.key.as_deref() == Some(k));
    let has_template = |ids: &[&str]| {
        items
            .iter()
            .any(|i| i.template.as_deref().is_some_and(|t| ids.contains(&t)))
    };
    let heating = profile.and_then(|p| p.heating.clone()).unwrap_or_default();
    let gas = heating.contains(&Heating::GasFurnace)
        || heating.contains(&Heating::Boiler)
        || profile.is_some_and(|p| p.water_heater == Some(WaterHeaterKind::TankGas))
        || has_template(&["furnace", "boiler"]);
    let well = profile.is_some_and(|p| p.water_supply == Some(WaterSupply::Well));
    let septic = profile.is_some_and(|p| p.sewer == Some(Sewer::Septic));
    let ducted = has_template(&["furnace", "hvac-air-filter", "heat-pump", "central-ac"])
        || heating.contains(&Heating::GasFurnace)
        || heating.contains(&Heating::HeatPump);

    let mut all = vec![
        prompt(
            "water_main",
            "shutoffs",
            "Water main shutoff",
            "Where is your water main shutoff?",
            "Basement, front wall, behind the boiler",
            false,
        ),
        prompt(
            "electrical_panel",
            "shutoffs",
            "Electrical panel",
            "Where is the electrical panel?",
            "Garage, left of the door",
            false,
        ),
    ];
    if gas {
        all.push(prompt(
            "gas_shutoff",
            "shutoffs",
            "Gas shutoff",
            "Where is the gas shutoff?",
            "At the meter, outside on the north side",
            false,
        ));
    }
    if well {
        all.push(prompt(
            "well_pressure_switch",
            "shutoffs",
            "Well pump switch",
            "Where is the well pump's switch or breaker?",
            "Breaker 14, labelled \"well\"",
            false,
        ));
    }
    if septic {
        all.push(prompt(
            "septic_lid",
            "shutoffs",
            "Septic tank lid",
            "Where is the septic tank lid?",
            "Back yard, 10 ft from the deck stairs",
            false,
        ));
    }
    if ducted {
        all.push(prompt(
            "hvac_filter_size",
            "sizes",
            "Furnace filter size",
            "What size is the furnace or air filter?",
            "16 x 25 x 1",
            false,
        ));
    }
    all.push(prompt(
        "smoke_alarm_battery",
        "sizes",
        "Smoke alarm battery",
        "What battery do the smoke alarms take?",
        "9V, or sealed 10-year",
        false,
    ));
    all.push(prompt(
        "wifi",
        "access",
        "Wi-Fi network and password",
        "What is the Wi-Fi network and password?",
        "HomeNet / ••••••••",
        true,
    ));
    all.push(prompt(
        "electric_provider",
        "utilities",
        "Electricity provider and account",
        "Who supplies electricity, and what is the account number?",
        "Provider, account number",
        true,
    ));
    all.retain(|p| !answered(p.key));
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PropertyId;

    #[test]
    fn codes_and_access_are_sensitive_by_default() {
        let s = Sensitivity::Sensitive;
        let n = Sensitivity::Normal;
        assert_eq!(default_sensitivity("access", "Front door"), s);
        assert_eq!(default_sensitivity("utilities", "Water provider"), s);
        assert_eq!(default_sensitivity("other", "Gate code"), s);
        assert_eq!(default_sensitivity("other", "Alarm panel PIN"), s);
        assert_eq!(default_sensitivity("other", "Safe combination"), s);
        assert_eq!(default_sensitivity("sizes", "Wi-Fi password"), s);
        assert_eq!(default_sensitivity("shutoffs", "Water main shutoff"), n);
        assert_eq!(default_sensitivity("sizes", "Furnace filter size"), n);
        assert_eq!(default_sensitivity("finishes", "Kitchen walls"), n);
        assert_eq!(default_sensitivity("shutoffs", "Spinning pump"), n);
    }

    #[test]
    fn prompts_follow_the_profile_and_stop_once_answered() {
        let pid = PropertyId::new();
        let none = prompts(None, &[], &[]);
        let keys: Vec<_> = none.iter().map(|p| p.key).collect();
        assert_eq!(keys[0], "water_main");
        assert!(
            !keys.contains(&"gas_shutoff"),
            "no profile, no gas question"
        );

        let mut p = Profile::empty(pid);
        p.heating = Some(vec![Heating::GasFurnace]);
        p.sewer = Some(Sewer::Septic);
        let keys: Vec<_> = prompts(Some(&p), &[], &[]).iter().map(|p| p.key).collect();
        assert!(keys.contains(&"gas_shutoff"));
        assert!(keys.contains(&"septic_lid"));
        assert!(keys.contains(&"hvac_filter_size"));

        let answered = Fact {
            pin: None,
            id: crate::ids::FactId::new(),
            property: pid,
            category: "shutoffs".into(),
            label: "Water main shutoff".into(),
            value: "Basement".into(),
            sensitivity: Sensitivity::Normal,
            key: Some("water_main".into()),
            room: None,
        };
        let keys: Vec<_> = prompts(Some(&p), &[], &[answered])
            .iter()
            .map(|p| p.key)
            .collect();
        assert!(!keys.contains(&"water_main"));
        for pr in prompts(Some(&p), &[], &[]) {
            assert_eq!(
                pr.sensitive,
                default_sensitivity(pr.group, pr.label) == Sensitivity::Sensitive,
                "{}",
                pr.key
            );
        }
    }
}
