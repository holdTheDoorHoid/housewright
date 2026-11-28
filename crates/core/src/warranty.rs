//! Warranties (D-27, P11): when one ends, and whether it is still running.
//!
//! Pure: no clock and no storage; "today" is a parameter.
//!
//! A warranty's end is stored on the item (`Warranty::expires`). When it was
//! given as a length ("5 years"), `Warranty::months` keeps that length and
//! the end is worked out from the day the warranty started: the earlier of
//! the install and purchase dates, because a warranty usually runs from
//! whichever came first, and a reminder that comes a little early is better
//! than one that comes after the cover has gone.

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::model::{Item, Warranty};

/// Within this many days of its end a warranty counts as "ending soon".
pub const ENDING_SOON_DAYS: i64 = 60;

/// Where a warranty stands on a given day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarrantyState {
    Active,
    EndingSoon,
    Ended,
}

/// `d` plus a number of whole months, clamped to the month's last day
/// (D-50: Jan 31 + 1 month = Feb 28 or 29).
pub fn add_months(d: Date, months: u32) -> Option<Date> {
    d.checked_add(Span::new().try_months(i64::from(months)).ok()?)
        .ok()
}

/// The day a warranty started: the earlier of install and purchase.
pub fn start(item: &Item) -> Option<Date> {
    match (item.installed, item.purchased) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// When the item's warranty ends, if it has one: worked out from its
/// length when that is known along with a start date, otherwise the
/// stored end date.
pub fn end(item: &Item) -> Option<Date> {
    let w = item.warranty.as_ref()?;
    match (w.months, start(item)) {
        (Some(m), Some(s)) => add_months(s, m).or(Some(w.expires)),
        _ => Some(w.expires),
    }
}

/// Bring the stored end date in line with the length and start date (call
/// after the install or purchase date or the length changes).
pub fn refresh(item: &mut Item) {
    if let Some(e) = end(item)
        && let Some(w) = item.warranty.as_mut()
    {
        w.expires = e;
    }
}

/// A warranty from a length or an end date. A length needs a start date
/// (install or purchase); an end date stands alone.
pub fn from_input(
    item: &Item,
    months: Option<u32>,
    ends: Option<Date>,
) -> Result<Option<Warranty>, String> {
    let old = item.warranty.clone();
    let keep = |expires: Date, months: Option<u32>| Warranty {
        expires,
        months,
        register_url: old.as_ref().and_then(|w| w.register_url.clone()),
        provider: old.as_ref().and_then(|w| w.provider.clone()),
        transferable: old.as_ref().is_some_and(|w| w.transferable),
        reference: old.as_ref().and_then(|w| w.reference.clone()),
    };
    match (months, ends) {
        (Some(0), _) => Err("a warranty of 0 months has already ended".into()),
        (Some(m), _) if m > 50 * 12 => Err("that warranty is longer than 50 years".into()),
        (Some(m), _) => {
            let s = start(item).ok_or_else(|| {
                "add the date it was bought or installed, so the end can be worked out".to_string()
            })?;
            let e = add_months(s, m).ok_or_else(|| "that date is out of range".to_string())?;
            Ok(Some(keep(e, Some(m))))
        }
        (None, Some(e)) => Ok(Some(keep(e, None))),
        (None, None) => Ok(None),
    }
}

/// Where a warranty that ends on `end` stands on `today`. The end day
/// itself is still covered.
pub fn state(end: Date, today: Date) -> WarrantyState {
    let left = today.until(end).map(|s| s.get_days()).unwrap_or(0);
    if end < today {
        WarrantyState::Ended
    } else if i64::from(left) <= ENDING_SOON_DAYS {
        WarrantyState::EndingSoon
    } else {
        WarrantyState::Active
    }
}

/// Whole days from `today` to `end` (negative once it has passed).
pub fn days_left(end: Date, today: Date) -> i64 {
    i64::from(today.until(end).map(|s| s.get_days()).unwrap_or(0))
}

/// Whether a link may be stored and shown as a registration page: https
/// only, no spaces, a host with a dot. Anything else is refused rather
/// than opened.
pub fn is_safe_link(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    url.len() <= 500
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
        && host.contains('.')
        && !host.contains('@')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ItemId, PropertyId};
    use jiff::civil::date;

    fn item(installed: Option<Date>, purchased: Option<Date>, w: Option<Warranty>) -> Item {
        Item {
            id: ItemId::new(),
            property: PropertyId::new(),
            room: None,
            parent: None,
            name: "Furnace".into(),
            category: None,
            make: None,
            model: None,
            serial: None,
            purchased,
            installed,
            warranty: w,
            location_note: None,
            template: None,
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Default::default(),
            price: None,
            value: None,
        }
    }

    fn w(expires: Date, months: Option<u32>) -> Warranty {
        Warranty {
            expires,
            months,
            register_url: None,
            provider: None,
            transferable: false,
            reference: None,
        }
    }

    #[test]
    fn length_counts_from_the_earlier_date() {
        let i = item(
            Some(date(2021, 3, 10)),
            Some(date(2021, 2, 28)),
            Some(w(date(2000, 1, 1), Some(60))),
        );
        assert_eq!(end(&i), Some(date(2026, 2, 28)));
    }

    #[test]
    fn month_ends_clamp() {
        assert_eq!(add_months(date(2024, 1, 31), 1), Some(date(2024, 2, 29)));
        assert_eq!(add_months(date(2023, 1, 31), 1), Some(date(2023, 2, 28)));
        assert_eq!(add_months(date(2020, 2, 29), 12), Some(date(2021, 2, 28)));
    }

    #[test]
    fn stored_date_when_no_length_or_no_start() {
        let i = item(None, None, Some(w(date(2027, 5, 1), Some(12))));
        assert_eq!(end(&i), Some(date(2027, 5, 1)));
        let i = item(Some(date(2020, 1, 1)), None, Some(w(date(2027, 5, 1), None)));
        assert_eq!(end(&i), Some(date(2027, 5, 1)));
        assert_eq!(end(&item(None, None, None)), None);
    }

    #[test]
    fn refresh_follows_a_new_purchase_date() {
        let mut i = item(None, Some(date(2025, 6, 1)), None);
        i.warranty = from_input(&i, Some(24), None).unwrap();
        assert_eq!(i.warranty.as_ref().unwrap().expires, date(2027, 6, 1));
        i.purchased = Some(date(2025, 7, 15));
        refresh(&mut i);
        assert_eq!(i.warranty.as_ref().unwrap().expires, date(2027, 7, 15));
    }

    #[test]
    fn input_rules() {
        let none = item(None, None, None);
        assert!(from_input(&none, Some(12), None).is_err());
        assert!(from_input(&none, Some(0), Some(date(2027, 1, 1))).is_err());
        assert_eq!(
            from_input(&none, None, Some(date(2027, 1, 1)))
                .unwrap()
                .unwrap()
                .expires,
            date(2027, 1, 1)
        );
        assert_eq!(from_input(&none, None, None).unwrap(), None);
    }

    #[test]
    fn states() {
        let today = date(2026, 11, 28);
        assert_eq!(state(date(2027, 6, 1), today), WarrantyState::Active);
        assert_eq!(state(date(2027, 1, 27), today), WarrantyState::EndingSoon);
        assert_eq!(state(today, today), WarrantyState::EndingSoon);
        assert_eq!(state(date(2026, 11, 27), today), WarrantyState::Ended);
        assert_eq!(days_left(date(2026, 12, 5), today), 7);
    }

    #[test]
    fn links() {
        assert!(is_safe_link("https://www.example.com/register"));
        assert!(!is_safe_link("http://www.example.com/"));
        assert!(!is_safe_link("javascript:alert(1)"));
        assert!(!is_safe_link("https://user@evil.example/"));
        assert!(!is_safe_link("https://exa mple.com/"));
        assert!(!is_safe_link("https://localhost/"));
    }
}
