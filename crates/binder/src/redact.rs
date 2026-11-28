//! The main volume's view of the house: everything marked sensitive taken
//! out (D-41).
//!
//! The main-volume builders take a [`Public`], and the only way to make one
//! is [`Public::of`], which removes every sensitive value before a builder
//! can see it. A builder therefore cannot print a sensitive value by
//! mistake; at most it can say how many were left out (the counts in
//! [`Withheld`]) and point to the sealed volume.

use housewright_core::model::{EntityKind, Sensitivity};

use crate::data::{HouseData, Withheld};

fn sensitive(s: Sensitivity) -> bool {
    s == Sensitivity::Sensitive
}

/// House data with nothing sensitive in it.
#[derive(Clone, Debug)]
pub struct Public(HouseData);

impl Public {
    pub fn data(&self) -> &HouseData {
        &self.0
    }

    /// Copy `full`, dropping every sensitive value and counting what went.
    pub fn of(full: &HouseData) -> Public {
        let mut d = full.clone();
        let mut w = Withheld::default();

        if d.property
            .address
            .as_ref()
            .is_some_and(|a| sensitive(a.sensitivity))
        {
            d.property.address = None;
        }
        if let Some(p) = &mut d.profile {
            // The ZIP is never printed; drop it whatever its flag.
            p.zip = None;
        }

        // Whole things marked sensitive (a safe, jewellery) go entirely.
        let hidden_items: Vec<String> = d
            .items
            .iter()
            .filter(|i| sensitive(i.item.sensitivity))
            .map(|i| i.item.id.to_string())
            .collect();
        w.items = hidden_items.len();
        d.items.retain(|i| !sensitive(i.item.sensitivity));
        for i in &mut d.items {
            let it = &mut i.item;
            if it.serial.as_ref().is_some_and(|s| sensitive(s.sensitivity)) {
                it.serial = None;
                w.item_details += 1;
            }
            if it
                .location_note
                .as_ref()
                .is_some_and(|s| sensitive(s.sensitivity))
            {
                it.location_note = None;
                w.item_details += 1;
            }
            if let Some(wr) = &mut it.warranty
                && wr
                    .reference
                    .as_ref()
                    .is_some_and(|r| sensitive(r.sensitivity))
            {
                wr.reference = None;
                w.item_details += 1;
            }
            let defs = i.fields.clone();
            let before = it.custom.len();
            it.custom.retain(|key, v| {
                let def_sensitive = defs
                    .iter()
                    .any(|f| &f.key == key && sensitive(f.sensitivity));
                !sensitive(v.sensitivity) && !def_sensitive
            });
            w.item_details += before - it.custom.len();
            i.attachments.retain(|a| !sensitive(a.sensitivity));
        }

        let kept: Vec<String> = d.items.iter().map(|i| i.item.id.to_string()).collect();
        d.completions.retain(|c| match c.item {
            Some(i) => kept.contains(&i.to_string()),
            None => true,
        });
        for c in &mut d.completions {
            if c.notes.as_ref().is_some_and(|n| sensitive(n.sensitivity)) {
                c.notes = None;
            }
        }

        for f in d.facts.iter().filter(|f| sensitive(f.fact.sensitivity)) {
            *w.facts.entry(f.fact.category.clone()).or_default() += 1;
        }
        d.facts.retain(|f| !sensitive(f.fact.sensitivity));
        for f in &mut d.facts {
            f.attachments.retain(|a| !sensitive(a.sensitivity));
        }

        for c in &mut d.contractors {
            c.private_rating = None;
            if c.notes.as_ref().is_some_and(|n| sensitive(n.sensitivity)) {
                c.notes = None;
                w.contractor_details += 1;
            }
            for cred in [&mut c.licence, &mut c.insurance] {
                if cred
                    .as_ref()
                    .is_some_and(|x| sensitive(x.number.sensitivity))
                {
                    *cred = None;
                    w.contractor_details += 1;
                }
            }
        }

        let before = d.notes.len();
        d.notes.retain(|n| {
            !sensitive(n.text.sensitivity)
                && !(n.about.kind == EntityKind::Item && hidden_items.contains(&n.about.id))
        });
        w.notes = before - d.notes.len();

        d.withheld = w;
        d.recovery = crate::data::Recovery::SyncOff;
        Public(d)
    }
}
