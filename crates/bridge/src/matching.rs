//! Which template a Home Assistant device looks like, from its name,
//! manufacturer and model. Deliberately simple: a person picks what to
//! import and can change the guess, so a miss costs a click, not data.

use crate::dto::DeviceIn;

/// Words (in the device's name or model) that point at a template.
/// More specific phrases come first.
const KEYWORDS: &[(&str, &str)] = &[
    ("heat pump water heater", "heat-pump-water-heater"),
    ("tankless", "tankless-water-heater"),
    ("water heater", "water-heater"),
    ("dishwasher", "dishwasher"),
    ("washing machine", "washing-machine"),
    ("washer", "washing-machine"),
    ("dryer", "clothes-dryer"),
    ("refrigerator", "refrigerator"),
    ("fridge", "refrigerator"),
    ("smoke", "smoke-alarm"),
    ("carbon monoxide", "co-alarm"),
    ("co alarm", "co-alarm"),
    ("co detector", "co-alarm"),
    ("heat pump", "heat-pump"),
    ("furnace", "furnace"),
    ("boiler", "boiler"),
    ("air conditioner", "central-ac"),
    ("garage door", "garage-door-opener"),
    ("sump", "sump-pump"),
    ("dehumidifier", "dehumidifier"),
    ("water softener", "water-softener"),
    ("softener", "water-softener"),
    ("generator", "generator"),
    ("solar", "solar-panels"),
    ("inverter", "solar-panels"),
    ("mower", "lawn-mower"),
    ("irrigation", "irrigation"),
    ("sprinkler", "irrigation"),
    ("pool", "pool"),
    ("range hood", "range-hood"),
    ("water filter", "whole-house-water-filter"),
    ("shutoff", "main-water-shutoff"),
    ("shut-off", "main-water-shutoff"),
    ("water valve", "main-water-shutoff"),
    ("well pump", "well-pump"),
    ("septic", "septic-system"),
];

/// The template id for a device, if any looks right. `templates` is the
/// library as (id, name); only ids in it are returned.
pub fn match_template(d: &DeviceIn, templates: &[(String, String)]) -> Option<String> {
    let text = format!(" {} {} ", d.name, d.model.as_deref().unwrap_or_default())
        .to_lowercase()
        .replace(['_', '-', '.', ','], " ");
    let has = |id: &str| templates.iter().any(|(t, _)| t == id);
    // A template's own name ("Washing machine") first.
    let mut by_name: Vec<&(String, String)> = templates
        .iter()
        .filter(|(_, n)| text.contains(&format!(" {} ", n.to_lowercase())))
        .collect();
    by_name.sort_by_key(|(_, n)| std::cmp::Reverse(n.len()));
    if let Some((id, _)) = by_name.first() {
        return Some(id.clone());
    }
    KEYWORDS
        .iter()
        .find(|(word, id)| text.contains(&format!(" {word}")) && has(id))
        .map(|(_, id)| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str, model: &str) -> DeviceIn {
        DeviceIn {
            id: "x".into(),
            name: name.into(),
            manufacturer: Some("Acme".into()),
            model: Some(model.into()),
            area: None,
            template: None,
        }
    }

    fn lib() -> Vec<(String, String)> {
        [
            ("washing-machine", "Washing machine"),
            ("clothes-dryer", "Clothes dryer"),
            ("smoke-alarm", "Smoke alarm"),
            ("water-heater", "Tank water heater"),
            ("heat-pump-water-heater", "Heat pump water heater"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
    }

    #[test]
    fn matches_by_name_and_keyword() {
        assert_eq!(
            match_template(&dev("Washer", "WM4000HWA"), &lib()).as_deref(),
            Some("washing-machine")
        );
        assert_eq!(
            match_template(&dev("Hallway smoke detector", "Nest Protect"), &lib()).as_deref(),
            Some("smoke-alarm")
        );
        assert_eq!(
            match_template(&dev("Basement", "Heat pump water heater HPWH50"), &lib()).as_deref(),
            Some("heat-pump-water-heater")
        );
        assert_eq!(
            match_template(&dev("Tank water heater", ""), &lib()).as_deref(),
            Some("water-heater")
        );
        assert_eq!(match_template(&dev("Kitchen bulb", "LCT015"), &lib()), None);
        // A keyword whose template is not in the library gives nothing.
        assert_eq!(match_template(&dev("Pool pump", ""), &lib()), None);
    }
}
