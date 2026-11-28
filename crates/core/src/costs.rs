//! Costs (D-27): what maintenance cost, per thing, per property and per
//! calendar year. Costs are optional and added after Done (P1), as
//! [`CompletionDetail`] records; a completion's own `cost` (older data or
//! another program) counts as "other".

use std::collections::BTreeMap;

use jiff::tz::TimeZone;
use serde::Serialize;

use crate::error::Result;
use crate::ids::{CompletionId, ItemId, PropertyId};
use crate::model::{
    Completion, CompletionDetail, CostKind, CostLine, Household, Item, Money, Property, Task,
};
use crate::schedule::civil_date;
use crate::store::Store;

/// Amounts in minor units, split by kind.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Split {
    pub parts: i64,
    pub labour: i64,
    pub contractor: i64,
    pub other: i64,
    pub total: i64,
}

impl Split {
    fn add(&mut self, kind: CostKind, minor: i64) {
        match kind {
            CostKind::Parts => self.parts += minor,
            CostKind::Labour => self.labour += minor,
            CostKind::Contractor => self.contractor += minor,
            CostKind::Other => self.other += minor,
        }
        self.total += minor;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Row<K> {
    pub key: K,
    pub label: String,
    pub split: Split,
    /// How many completions had a cost.
    pub jobs: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostSummary {
    pub currency: String,
    pub total: Split,
    pub by_year: Vec<Row<i16>>,
    pub by_item: Vec<Row<Option<ItemId>>>,
    pub by_property: Vec<Row<PropertyId>>,
    /// Lines in another currency, left out of the sums rather than mixed in.
    pub other_currency_lines: usize,
}

/// The cost lines that count for one completion: the latest detail that
/// set costs wins (a correction is a newer detail); otherwise the
/// completion's own cost.
pub fn effective_costs(c: &Completion, details: &[CompletionDetail]) -> Vec<CostLine> {
    let latest = details
        .iter()
        .filter(|d| d.completion == c.id && d.costs.is_some())
        .max_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    match latest {
        Some(d) => d.costs.clone().unwrap_or_default(),
        None => c
            .cost
            .clone()
            .map(|amount| {
                vec![CostLine {
                    kind: CostKind::Other,
                    amount,
                }]
            })
            .unwrap_or_default(),
    }
}

/// Completions that still count (not voided, not voiding records).
pub(crate) fn effective(all: Vec<Completion>) -> Vec<Completion> {
    let voided: Vec<CompletionId> = all.iter().filter_map(|c| c.voids).collect();
    all.into_iter()
        .filter(|c| c.voids.is_none() && !voided.contains(&c.id))
        .collect()
}

/// Sum every cost in the household.
pub fn summarise(store: &Store) -> Result<CostSummary> {
    let household: Household = store.household()?;
    let currency = household.currency().to_string();
    let props: BTreeMap<PropertyId, Property> = store
        .list::<Property>()?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
    let tasks: BTreeMap<_, Task> = store
        .list::<Task>()?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    let items: BTreeMap<ItemId, Item> = store
        .list::<Item>()?
        .into_iter()
        .map(|i| (i.id, i))
        .collect();
    let details: Vec<CompletionDetail> = store.list()?;
    let mut total = Split::default();
    let mut by_year: BTreeMap<i16, (Split, usize)> = BTreeMap::new();
    let mut by_item: BTreeMap<Option<ItemId>, (Split, usize)> = BTreeMap::new();
    let mut by_prop: BTreeMap<PropertyId, (Split, usize)> = BTreeMap::new();
    let mut other_currency_lines = 0;
    for c in effective(store.list::<Completion>()?) {
        let Some(task) = tasks.get(&c.task) else {
            continue;
        };
        let lines = effective_costs(&c, &details);
        let lines: Vec<&CostLine> = lines
            .iter()
            .filter(|l| {
                let same = l.amount.currency == currency;
                if !same {
                    other_currency_lines += 1;
                }
                same
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        let tz = props
            .get(&task.property)
            .and_then(|p| TimeZone::get(&p.time_zone).ok())
            .unwrap_or(TimeZone::UTC);
        let year = civil_date(c.at, &tz).year();
        for entry in [
            by_year.entry(year).or_default(),
            by_item.entry(task.item).or_default(),
            by_prop.entry(task.property).or_default(),
        ] {
            for l in &lines {
                entry.0.add(l.kind, l.amount.minor);
            }
            entry.1 += 1;
        }
        for l in &lines {
            total.add(l.kind, l.amount.minor);
        }
    }
    let mut by_item: Vec<Row<Option<ItemId>>> = by_item
        .into_iter()
        .map(|(k, (split, jobs))| Row {
            key: k,
            label: k
                .and_then(|i| items.get(&i).map(|i| i.name.clone()))
                .unwrap_or_else(|| "Not about one thing".into()),
            split,
            jobs,
        })
        .collect();
    by_item.sort_by(|a, b| {
        b.split
            .total
            .cmp(&a.split.total)
            .then(a.label.cmp(&b.label))
    });
    Ok(CostSummary {
        currency,
        total,
        by_year: by_year
            .into_iter()
            .rev()
            .map(|(k, (split, jobs))| Row {
                key: k,
                label: k.to_string(),
                split,
                jobs,
            })
            .collect(),
        by_item,
        by_property: by_prop
            .into_iter()
            .map(|(k, (split, jobs))| Row {
                key: k,
                label: props
                    .get(&k)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "A home".into()),
                split,
                jobs,
            })
            .collect(),
        other_currency_lines,
    })
}

/// Parse a typed amount ("12", "12.5", "$1,299.99") into minor units for a
/// two-decimal currency. Negative amounts and more than two decimals are
/// refused.
pub fn parse_amount(text: &str) -> Option<i64> {
    let t: String = text
        .trim()
        .chars()
        .filter(|c| !matches!(c, '$' | ',' | ' ' | '€' | '£'))
        .collect();
    if t.is_empty() || t.starts_with('-') {
        return None;
    }
    let (whole, frac) = match t.split_once('.') {
        Some((w, f)) => (w, f),
        None => (t.as_str(), ""),
    };
    if frac.len() > 2
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || (whole.is_empty() && frac.is_empty())
    {
        return None;
    }
    let w: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let f: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse().ok()?,
    };
    w.checked_mul(100)?.checked_add(f)
}

/// Write minor units back out: `129999` -> `"1,299.99"`.
pub fn format_amount(minor: i64) -> String {
    let neg = minor < 0;
    let m = minor.unsigned_abs();
    let whole = (m / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}{grouped}.{:02}", if neg { "-" } else { "" }, m % 100)
}

pub fn money(minor: i64, currency: &str) -> Money {
    Money {
        minor,
        currency: currency.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts() {
        assert_eq!(parse_amount("12"), Some(1200));
        assert_eq!(parse_amount("12.5"), Some(1250));
        assert_eq!(parse_amount("$1,299.99"), Some(129999));
        assert_eq!(parse_amount(".99"), Some(99));
        assert_eq!(parse_amount("1.999"), None);
        assert_eq!(parse_amount("-4"), None);
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(parse_amount(""), None);
        assert_eq!(format_amount(129999), "1,299.99");
        assert_eq!(format_amount(5), "0.05");
        assert_eq!(format_amount(100000000), "1,000,000.00");
    }
}
