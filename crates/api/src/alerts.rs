//! Phase 9 commands: warranties, dated notices in the reminder plan, and
//! product recall checks on the device.
//!
//! Recall checks never send anything about the household anywhere. The
//! front end downloads the public CPSC list by date range
//! (`recalls().next`, built by `housewright_core::recalls::cpsc_url`) and
//! hands the response to `recall_import`; matching happens here.

use std::collections::{BTreeMap, BTreeSet};

use housewright_core::alerts::{self as notices, NoticeTarget};
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::recalls::{self, MatchLevel, Recall, Window};
use housewright_core::reminders::{self, ReminderKind, ReminderPrefs};
use housewright_core::warranty::{self, WarrantyState};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

const RECALLS_ON: &str = "recalls_on";
const RECALLS_SYNCED_TO: &str = "recalls_synced_to";
const RECALLS_CHECKED_AT: &str = "recalls_checked_at";
const RECALLS_ALERTS: &str = "recalls_alerts";

/// A warranty as typed on a thing's edit form.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct WarrantyIn {
    /// Remove the warranty.
    #[serde(default)]
    pub remove: bool,
    /// Its length in months ("5 years" = 60), counted from the earlier of
    /// install and purchase.
    #[serde(default)]
    pub months: Option<u32>,
    /// Or the end date itself.
    #[serde(default)]
    pub ends: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub register_url: Option<String>,
    #[serde(default)]
    pub transferable: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WarrantyOut {
    pub ends: Date,
    pub months: Option<u32>,
    pub state: WarrantyState,
    pub days_left: i64,
    pub provider: Option<String>,
    pub transferable: bool,
    pub register_url: Option<String>,
    /// Whether a reference is stored (it may be sensitive; not sent).
    pub has_reference: bool,
}

pub(crate) fn warranty_out(i: &Item, today: Date) -> Option<WarrantyOut> {
    let w = i.warranty.as_ref()?;
    let ends = warranty::end(i).unwrap_or(w.expires);
    Some(WarrantyOut {
        ends,
        months: w.months,
        state: warranty::state(ends, today),
        days_left: warranty::days_left(ends, today),
        provider: w.provider.clone(),
        transferable: w.transferable,
        register_url: w.register_url.clone(),
        has_reference: w.reference.is_some(),
    })
}

#[derive(Serialize)]
pub struct WarrantyRow {
    pub item_id: String,
    pub name: String,
    pub room_name: Option<String>,
    #[serde(flatten)]
    pub warranty: WarrantyOut,
}

#[derive(Serialize)]
pub struct WarrantiesOut {
    pub today: Date,
    pub ending_soon: Vec<WarrantyRow>,
    pub active: Vec<WarrantyRow>,
    pub ended: Vec<WarrantyRow>,
    /// Things with no warranty recorded.
    pub without: usize,
}

#[derive(Serialize)]
pub struct WindowOut {
    pub start: Date,
    pub end: Date,
    /// The whole request: dates and format only.
    pub url: String,
}

#[derive(Serialize)]
pub struct RecallOut {
    pub number: String,
    pub date: Date,
    pub title: String,
    pub url: String,
    pub hazard: String,
    pub remedy: String,
    pub products: Vec<String>,
}

#[derive(Serialize)]
pub struct RecallMatchOut {
    pub item_id: String,
    pub item_name: String,
    pub room_name: Option<String>,
    pub level: MatchLevel,
    pub evidence: Vec<String>,
    pub recall: RecallOut,
    /// Set for a "not mine" answer, to undo it.
    pub dismissal_id: Option<String>,
}

#[derive(Serialize)]
pub struct RecallsOut {
    pub enabled: bool,
    /// The last day the downloaded list covers.
    pub synced_to: Option<Date>,
    pub checked_at: Option<jiff::Timestamp>,
    /// How many recalls are kept on this device.
    pub stored: usize,
    pub matches: Vec<RecallMatchOut>,
    pub dismissed: Vec<RecallMatchOut>,
    /// What to download next (empty when up to date today).
    pub next: Vec<WindowOut>,
    /// Wait this long between requests.
    pub pause_ms: u64,
}

#[derive(Deserialize)]
pub struct OnArgs {
    pub on: bool,
}

#[derive(Deserialize)]
pub struct ImportRecallsArgs {
    /// The response body, as downloaded.
    pub json: String,
    pub start: Date,
    pub end: Date,
}

#[derive(Serialize)]
pub struct ImportRecallsOut {
    pub received: usize,
    pub stored: usize,
}

#[derive(Deserialize)]
pub struct DismissArgs {
    pub item_id: String,
    pub number: String,
}

impl Api {
    pub(crate) fn alerts_call(&mut self, cmd: &str, a: Value) -> Option<Result<Value, ApiError>> {
        Some(match cmd {
            "warranties" => self.warranties().and_then(ok),
            "insurance" => self.insurance_summary().and_then(ok),
            "recalls" => self.recalls().and_then(ok),
            "set_recalls" => args(a).and_then(|a: OnArgs| {
                if a.on {
                    self.store.set_meta(RECALLS_ON, "1")?;
                } else {
                    // Off: forget the downloaded list too, so turning it on
                    // again starts fresh and nothing lingers.
                    self.store.set_meta(RECALLS_ON, "0")?;
                    self.store.set_meta(RECALLS_SYNCED_TO, "")?;
                    self.store.set_meta(RECALLS_CHECKED_AT, "")?;
                    self.store.set_meta(RECALLS_ALERTS, "0")?;
                    self.store.recall_cache_clear()?;
                }
                ok(self.recalls()?)
            }),
            "recall_import" => args(a).and_then(|a: ImportRecallsArgs| ok(self.recall_import(a)?)),
            "dismiss_recall" => args(a).and_then(|a: DismissArgs| {
                let item: ItemId = parse_id(&a.item_id, "item")?;
                let _: Item = self.store.fetch(&item.to_string())?;
                let number = a.number.trim().to_string();
                if number.is_empty() || number.len() > 20 {
                    return Err(ApiError::invalid("not a recall number".to_string()));
                }
                let me = self.store.author()?;
                let now = self.store.now();
                self.store.append(&RecallDismissal {
                    id: RecallDismissalId::new(),
                    item,
                    recall: number,
                    by: me,
                    at: now,
                })?;
                ok(self.recalls()?)
            }),
            "undismiss_recall" => args(a).and_then(|a: IdArgs| {
                let id: RecallDismissalId = parse_id(&a.id, "answer")?;
                let _: RecallDismissal = self.store.fetch(&id.to_string())?;
                self.store
                    .delete(EntityKind::RecallDismissal, &id.to_string())?;
                ok(self.recalls()?)
            }),
            _ => return None,
        })
    }

    fn room_names(&self) -> Result<BTreeMap<RoomId, String>, ApiError> {
        Ok(self
            .store
            .list::<Room>()?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect())
    }

    fn warranties(&self) -> Result<WarrantiesOut, ApiError> {
        let rooms = self.room_names()?;
        let see = self.me()?.role.can_see_sensitive();
        let p = self.first_property()?;
        let today = self.today_in(&p.time_zone)?;
        let mut out = WarrantiesOut {
            today,
            ending_soon: vec![],
            active: vec![],
            ended: vec![],
            without: 0,
        };
        for i in self.store.list::<Item>()? {
            if i.sensitivity == Sensitivity::Sensitive && !see {
                continue;
            }
            let Some(w) = warranty_out(&i, self.item_today(&i)?) else {
                out.without += 1;
                continue;
            };
            let row = WarrantyRow {
                item_id: i.id.to_string(),
                name: i.name.clone(),
                room_name: i.room.and_then(|r| rooms.get(&r).cloned()),
                warranty: w,
            };
            match row.warranty.state {
                WarrantyState::EndingSoon => out.ending_soon.push(row),
                WarrantyState::Active => out.active.push(row),
                WarrantyState::Ended => out.ended.push(row),
            }
        }
        out.ending_soon.sort_by_key(|r| r.warranty.ends);
        out.active.sort_by_key(|r| r.warranty.ends);
        out.ended
            .sort_by_key(|r| std::cmp::Reverse(r.warranty.ends));
        Ok(out)
    }

    /// Warranty and contractor notices for this person (P5: placed in their
    /// maintenance window, never in quiet hours). A thing's warranty notice
    /// goes to whoever looks after its switched-on tasks (P8), or to the
    /// household's owners when nobody does; contractor notices go to the
    /// owners.
    pub(crate) fn notice_plan(
        &self,
        me: &Member,
        prefs: &ReminderPrefs,
        horizon_days: Option<i64>,
    ) -> Result<Vec<ReminderOut>, ApiError> {
        if !me.kind.is_person() {
            return Ok(vec![]);
        }
        let now = self.store.now();
        let p = self.first_property()?;
        let tz = TimeZone::get(&p.time_zone)?;
        let today = self.today_in(&p.time_zone)?;
        let horizon = add_days(
            today,
            horizon_days.unwrap_or(reminders::DEFAULT_HORIZON_DAYS),
        );
        let tasks: Vec<Task> = self.store.list()?;
        let owner = me.role == Role::Owner;
        let mine: Vec<Item> = self
            .store
            .list::<Item>()?
            .into_iter()
            .filter(|i| {
                let keepers: BTreeSet<MemberId> = tasks
                    .iter()
                    .filter(|t| t.item == Some(i.id) && t.active)
                    .map(|t| t.owner)
                    .collect();
                if keepers.is_empty() {
                    owner
                } else {
                    keepers.contains(&me.id)
                }
            })
            .collect();
        let rooms = self.room_names()?;
        let mut all = notices::warranty_notices(
            &mine,
            &|i: &Item| i.room.and_then(|r| rooms.get(&r).cloned()),
            today,
        );
        if owner {
            all.extend(notices::contractor_notices(
                &self.store.list::<Contractor>()?,
                today,
            ));
        }
        let mut out = Vec::new();
        for n in all {
            if n.date > horizon {
                continue;
            }
            let Some(at) = reminders::place(n.date, prefs, &tz) else {
                continue;
            };
            if at <= now {
                continue;
            }
            out.push(ReminderOut {
                task_id: n.key,
                kind: ReminderKind::Notice,
                at_ms: at.as_millisecond(),
                title: n.title,
                body: n.body,
                open: Some(match n.target {
                    NoticeTarget::Item(i) => format!("item/{i}"),
                    NoticeTarget::Contractor(c) => format!("contractor/{c}"),
                }),
            });
        }
        Ok(out)
    }

    // -- recalls -----------------------------------------------------------

    fn recalls_on(&self) -> Result<bool, ApiError> {
        Ok(self.store.meta(RECALLS_ON)?.as_deref() == Some("1"))
    }

    fn meta_date(&self, key: &str) -> Result<Option<Date>, ApiError> {
        Ok(self.store.meta(key)?.and_then(|s| s.parse().ok()))
    }

    fn cached_recalls(&self) -> Result<Vec<Recall>, ApiError> {
        Ok(self
            .store
            .recall_cache_all()?
            .into_iter()
            .filter_map(|(_, _, data)| serde_json::from_str(&data).ok())
            .collect())
    }

    /// How many possible recalls are waiting, as last worked out (cheap,
    /// for Today).
    pub(crate) fn recall_alert_count(&self) -> Result<usize, ApiError> {
        if !self.recalls_on()? {
            return Ok(0);
        }
        Ok(self
            .store
            .meta(RECALLS_ALERTS)?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    fn recalls(&self) -> Result<RecallsOut, ApiError> {
        let enabled = self.recalls_on()?;
        let synced_to = self.meta_date(RECALLS_SYNCED_TO)?;
        let checked_at = self
            .store
            .meta(RECALLS_CHECKED_AT)?
            .and_then(|s| s.parse().ok());
        let p = self.first_property()?;
        let today = self.today_in(&p.time_zone)?;
        let next = if !enabled || synced_to == Some(today) {
            vec![]
        } else {
            recalls::windows(synced_to, today)
                .into_iter()
                .map(|w| WindowOut {
                    start: w.start,
                    end: w.end,
                    url: recalls::cpsc_url(w),
                })
                .collect()
        };
        let cached = if enabled {
            self.cached_recalls()?
        } else {
            vec![]
        };
        let see = self.me()?.role.can_see_sensitive();
        let items: Vec<Item> = self
            .store
            .list::<Item>()?
            .into_iter()
            .filter(|i| see || i.sensitivity != Sensitivity::Sensitive)
            .collect();
        let dismissals: Vec<RecallDismissal> = self.store.list()?;
        let all = recalls::matches(&items, &cached, &BTreeSet::new());
        let by_number: BTreeMap<&str, &Recall> =
            cached.iter().map(|r| (r.number.as_str(), r)).collect();
        let rooms = self.room_names()?;
        let mut matches = Vec::new();
        let mut dismissed = Vec::new();
        for m in all {
            let Some(r) = by_number.get(m.number.as_str()) else {
                continue;
            };
            let Some(i) = items.iter().find(|i| i.id == m.item) else {
                continue;
            };
            let d = dismissals
                .iter()
                .find(|d| d.item == m.item && d.recall == m.number);
            let out = RecallMatchOut {
                item_id: i.id.to_string(),
                item_name: i.name.clone(),
                room_name: i.room.and_then(|r| rooms.get(&r).cloned()),
                level: m.level,
                evidence: m.evidence,
                recall: RecallOut {
                    number: r.number.clone(),
                    date: r.date,
                    title: r.title.clone(),
                    url: r.url.clone(),
                    hazard: r.hazard.clone(),
                    remedy: r.remedy.clone(),
                    products: r.products.clone(),
                },
                dismissal_id: d.map(|d| d.id.to_string()),
            };
            if d.is_some() {
                dismissed.push(out);
            } else {
                matches.push(out);
            }
        }
        // Remember the count for Today (a device setting, never synced).
        if enabled {
            let n = matches.len().to_string();
            if self.store.meta(RECALLS_ALERTS)?.as_deref() != Some(n.as_str()) {
                self.store.set_meta(RECALLS_ALERTS, &n)?;
            }
        }
        Ok(RecallsOut {
            enabled,
            synced_to,
            checked_at,
            stored: if enabled {
                self.store.recall_cache_all()?.len()
            } else {
                0
            },
            matches,
            dismissed,
            next,
            pause_ms: recalls::PAUSE_MS,
        })
    }

    fn recall_import(&mut self, a: ImportRecallsArgs) -> Result<ImportRecallsOut, ApiError> {
        if !self.recalls_on()? {
            return Err(ApiError::invalid(
                "recall checks are switched off on this device".to_string(),
            ));
        }
        if a.end < a.start {
            return Err(ApiError::invalid(
                "the dates are the wrong way round".to_string(),
            ));
        }
        let list = recalls::parse_cpsc(&a.json).map_err(ApiError::invalid)?;
        let rows: Vec<(String, String, String)> = list
            .iter()
            .filter_map(|r| {
                serde_json::to_string(r)
                    .ok()
                    .map(|j| (r.number.clone(), r.date.to_string(), j))
            })
            .collect();
        self.store.recall_cache_put(&rows)?;
        let w = Window {
            start: a.start,
            end: a.end,
        };
        // Only move forward: an older window never rewinds the mark.
        if self.meta_date(RECALLS_SYNCED_TO)?.is_none_or(|d| w.end > d) {
            self.store.set_meta(RECALLS_SYNCED_TO, &w.end.to_string())?;
        }
        self.store
            .set_meta(RECALLS_CHECKED_AT, &self.store.now().to_string())?;
        // Refresh the count for Today.
        let _ = self.recalls()?;
        Ok(ImportRecallsOut {
            received: list.len(),
            stored: self.store.recall_cache_all()?.len(),
        })
    }
}
