//! Dated notices that are not tasks (Phase 9): a warranty about to end, a
//! contractor's licence or insurance about to lapse.
//!
//! Pure: the caller passes "today" and places each notice in the person's
//! day with [`crate::reminders::place`] (maintenance window, quiet hours),
//! so notices obey the same rules as task reminders (P5). Each notice fires
//! once: a warranty gets one about 60 days and one about 7 days before it
//! ends; a licence or insurance certificate gets one 30 days before it
//! lapses, and only for contractors the household would hire again (asking
//! someone you would not hire again for their renewed licence is noise).
//!
//! Every notice comes with something to do (P9).

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::ids::{ContractorId, ItemId};
use crate::model::{Contractor, Item, Sensitivity};
use crate::text_date;
use crate::warranty;

/// Days before a warranty ends that a notice goes out.
pub const WARRANTY_LEADS: [i64; 2] = [60, 7];
/// Days before a licence or insurance certificate lapses.
pub const CREDENTIAL_LEAD: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    WarrantyEnding,
    LicenceExpiring,
    InsuranceExpiring,
}

/// What a notice opens when tapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum NoticeTarget {
    Item(ItemId),
    Contractor(ContractorId),
}

/// One dated notice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    /// Stable for one notice ("warranty:<item>:60"), so a platform can
    /// replace rather than repeat it.
    pub key: String,
    pub kind: NoticeKind,
    pub date: Date,
    pub title: String,
    pub body: String,
    pub target: NoticeTarget,
}

fn minus_days(d: Date, n: i64) -> Option<Date> {
    d.checked_sub(Span::new().try_days(n).ok()?).ok()
}

/// Warranty notices for these items, dated today or later. `room` gives
/// an item's room name, for the wording. An item marked sensitive (a safe)
/// gets a notice that does not name it, because notifications show on a
/// locked screen.
pub fn warranty_notices(
    items: &[Item],
    room: &dyn Fn(&Item) -> Option<String>,
    today: Date,
) -> Vec<Notice> {
    let mut out = Vec::new();
    for i in items {
        let Some(end) = warranty::end(i) else {
            continue;
        };
        if end < today {
            continue;
        }
        let private = i.sensitivity == Sensitivity::Sensitive;
        let name = if private {
            "Something you marked private".to_string()
        } else {
            i.name.clone()
        };
        let w = i.warranty.as_ref();
        for lead in WARRANTY_LEADS {
            let Some(date) = minus_days(end, lead) else {
                continue;
            };
            if date < today {
                continue;
            }
            let when = if lead >= 30 {
                format!("in {} days", lead)
            } else {
                "in a week".to_string()
            };
            let mut body = format!("Covered until {}.", text_date(end));
            if !private && let Some(r) = room(i) {
                body = format!("{}, {r}. {body}", i.name);
            }
            body.push_str(
                " Book a check-up while it is covered: anything wrong found now can still be fixed under the warranty.",
            );
            if w.is_some_and(|w| w.register_url.is_some()) {
                body.push_str(" If you have not registered it yet, the link is on its page.");
            }
            out.push(Notice {
                key: format!("warranty:{}:{lead}", i.id),
                kind: NoticeKind::WarrantyEnding,
                date,
                title: format!("{name}: warranty ends {when}"),
                body,
                target: NoticeTarget::Item(i.id),
            });
        }
    }
    out
}

/// Licence and insurance notices, only for contractors marked "would hire
/// again", dated today or later.
pub fn contractor_notices(contractors: &[Contractor], today: Date) -> Vec<Notice> {
    let mut out = Vec::new();
    for c in contractors {
        if c.would_hire_again != Some(true) {
            continue;
        }
        let who = c.company.clone().unwrap_or_else(|| c.name.clone());
        for (kind, cred, what, ask) in [
            (
                NoticeKind::LicenceExpiring,
                &c.licence,
                "licence",
                "Ask for the renewed licence number before you next book them.",
            ),
            (
                NoticeKind::InsuranceExpiring,
                &c.insurance,
                "insurance",
                "Ask for a current insurance certificate before they next work on the house, and add it to their page.",
            ),
        ] {
            let Some(end) = cred.as_ref().and_then(|x| x.expires) else {
                continue;
            };
            let Some(date) = minus_days(end, CREDENTIAL_LEAD) else {
                continue;
            };
            if date < today {
                continue;
            }
            out.push(Notice {
                key: format!("{what}:{}", c.id),
                kind,
                date,
                title: format!("{who}: {what} ends {}", text_date(end)),
                body: format!("You marked them as someone you would hire again. {ask}"),
                target: NoticeTarget::Contractor(c.id),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PropertyId;
    use crate::model::{Credential, Flagged, Warranty};
    use jiff::civil::date;

    fn item(name: &str, end: Date) -> Item {
        Item {
            id: ItemId::new(),
            property: PropertyId::new(),
            room: None,
            parent: None,
            name: name.into(),
            category: None,
            make: None,
            model: None,
            serial: None,
            purchased: None,
            installed: None,
            warranty: Some(Warranty {
                expires: end,
                months: None,
                register_url: Some("https://example.com/register".into()),
                provider: None,
                transferable: false,
                reference: None,
            }),
            location_note: None,
            template: None,
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Default::default(),
            price: None,
            value: None,
        }
    }

    #[test]
    fn two_warranty_notices_with_an_action() {
        let today = date(2026, 11, 28);
        let i = item("Furnace", date(2027, 3, 1));
        let n = warranty_notices(&[i], &|_| Some("Basement".into()), today);
        assert_eq!(n.len(), 2);
        assert_eq!(n[0].date, date(2026, 12, 31));
        assert_eq!(n[1].date, date(2027, 2, 22));
        assert_eq!(n[0].title, "Furnace: warranty ends in 60 days");
        assert_eq!(n[1].title, "Furnace: warranty ends in a week");
        assert!(n[0].body.starts_with("Furnace, Basement. Covered until Mar 1, 2027."));
        assert!(n[0].body.contains("Book a check-up"));
        assert!(n[0].body.contains("registered"));
    }

    #[test]
    fn passed_leads_are_skipped_and_ended_warranties_quiet() {
        let today = date(2026, 11, 28);
        // Ends in 20 days: only the 7-day notice is left.
        let n = warranty_notices(&[item("Dryer", date(2026, 12, 18))], &|_| None, today);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].date, date(2026, 12, 11));
        assert!(warranty_notices(&[item("Old", date(2026, 1, 1))], &|_| None, today).is_empty());
    }

    #[test]
    fn private_things_are_not_named() {
        let mut i = item("Wall safe", date(2027, 3, 1));
        i.sensitivity = Sensitivity::Sensitive;
        let n = warranty_notices(&[i], &|_| Some("Bedroom".into()), date(2026, 11, 28));
        assert!(n.iter().all(|x| !x.title.contains("safe") && !x.body.contains("safe")));
        assert!(n.iter().all(|x| !x.body.contains("Bedroom")));
    }

    #[test]
    fn credentials_only_for_use_again() {
        let today = date(2026, 11, 28);
        let mk = |again: Option<bool>| Contractor {
            id: ContractorId::new(),
            name: "Ana Ruiz".into(),
            company: Some("Ruiz Plumbing".into()),
            trades: vec![],
            phone: None,
            email: None,
            website: None,
            service_area: None,
            licence: Some(Credential {
                number: Flagged::normal("PL-1".into()),
                expires: Some(date(2027, 2, 1)),
            }),
            insurance: Some(Credential {
                number: Flagged::normal("INS-1".into()),
                expires: Some(date(2026, 12, 10)),
            }),
            private_rating: None,
            notes: None,
            emergency: false,
            would_hire_again: again,
            private_notes: None,
        };
        let n = contractor_notices(&[mk(Some(true)), mk(Some(false)), mk(None)], today);
        // Insurance lapses in 12 days: its 30-day notice has passed.
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].kind, NoticeKind::LicenceExpiring);
        assert_eq!(n[0].date, date(2027, 1, 2));
        assert_eq!(n[0].title, "Ruiz Plumbing: licence ends Feb 1, 2027");
    }
}
