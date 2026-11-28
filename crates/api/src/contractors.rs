//! Phase 9 commands: contractors (D-28): contacts and trades, licence and
//! insurance, the household's private opinion, and the jobs they did.
//!
//! The private fields (rating, "would hire again", private notes) are
//! shown to the household's owner and members, never to limited members,
//! and never leave the household's own screens: the binder drops them, the
//! Home Assistant bridge never reads contractors, and no server extra gets
//! them (D-16).

use std::collections::BTreeMap;

use housewright_core::contractors;
use housewright_core::ids::*;
use housewright_core::model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

/// A licence or insurance certificate within this many days of lapsing
/// shows as "ending soon".
const CREDENTIAL_SOON_DAYS: i64 = 30;

/// A contractor as saved. Fields marked "absent = unchanged" can be left
/// out by screens that do not show them (the binder's "Who to call").
#[derive(Deserialize)]
pub struct ContractorIn {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub trades: Vec<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Notes go in the sealed volume only.
    #[serde(default)]
    pub notes_private: bool,
    #[serde(default)]
    pub emergency: bool,
    // Absent = unchanged; "" clears.
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub service_area: Option<String>,
    #[serde(default)]
    pub licence_number: Option<String>,
    #[serde(default)]
    pub licence_expires: Option<String>,
    #[serde(default)]
    pub insurance_number: Option<String>,
    #[serde(default)]
    pub insurance_expires: Option<String>,
    /// 1 to 5; 0 clears. Absent = unchanged.
    #[serde(default)]
    pub private_rating: Option<u8>,
    /// "yes", "no" or "" (not said). Absent = unchanged.
    #[serde(default)]
    pub would_hire_again: Option<String>,
    #[serde(default)]
    pub private_notes: Option<String>,
}

#[derive(Serialize)]
pub struct CredentialOut {
    /// `None` when marked sensitive and the viewer may not see it.
    pub number: Option<String>,
    pub expires: Option<Date>,
    /// `ok`, `ending_soon`, `ended` or `no_date`.
    pub state: &'static str,
}

/// The household's own opinion. Never printed or shared.
#[derive(Serialize)]
pub struct PrivateOut {
    pub rating: Option<u8>,
    pub would_hire_again: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct ContractorOut {
    pub id: String,
    pub name: String,
    pub company: Option<String>,
    pub trades: Vec<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
    pub service_area: Option<String>,
    pub notes: Option<String>,
    pub notes_private: bool,
    pub emergency: bool,
    pub licence: Option<CredentialOut>,
    pub insurance: Option<CredentialOut>,
    /// Absent for limited members.
    pub private: Option<PrivateOut>,
    pub jobs: usize,
    /// What they were paid, in the household's currency (minor units).
    pub spent: i64,
}

#[derive(Serialize)]
pub struct JobOut {
    pub completion_id: String,
    pub task_id: String,
    pub task_title: String,
    pub at: jiff::Timestamp,
    pub item_id: Option<String>,
    pub item_name: Option<String>,
    pub room_name: Option<String>,
    pub costs: Vec<CostLineOut>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct ContractorPage {
    pub contractor: ContractorOut,
    pub jobs: Vec<JobOut>,
    pub currency: String,
    /// The insurance certificate and other files.
    pub attachments: Vec<AttachmentOut>,
}

fn blank(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// A website as typed: "example.com" becomes "https://example.com". Only
/// http and https links with a real host are kept.
pub(crate) fn website(s: &str) -> Result<Option<String>, ApiError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let full = if s.starts_with("https://") || s.starts_with("http://") {
        s.to_string()
    } else if s.contains("://") || s.contains(':') && !s.contains('.') {
        return Err(ApiError::invalid(
            "a website starts with https://".to_string(),
        ));
    } else {
        format!("https://{s}")
    };
    let check = full.replacen("http://", "https://", 1);
    if !housewright_core::warranty::is_safe_link(&check) {
        return Err(ApiError::invalid(format!(
            "'{s}' does not look like a website"
        )));
    }
    Ok(Some(full))
}

fn date_in(s: &str) -> Result<Option<Date>, ApiError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    s.parse()
        .map(Some)
        .map_err(|e| ApiError::invalid(format!("'{s}' is not a date: {e}")))
}

fn credential(
    old: Option<Credential>,
    number: Option<String>,
    expires: Option<String>,
) -> Result<Option<Credential>, ApiError> {
    if number.is_none() && expires.is_none() {
        return Ok(old);
    }
    let num = match number {
        Some(n) => n.trim().to_string(),
        None => old
            .as_ref()
            .map(|c| c.number.value.clone())
            .unwrap_or_default(),
    };
    let exp = match expires {
        Some(e) => date_in(&e)?,
        None => old.as_ref().and_then(|c| c.expires),
    };
    if num.is_empty() && exp.is_none() {
        return Ok(None);
    }
    Ok(Some(Credential {
        number: Flagged {
            value: num,
            sensitivity: old
                .as_ref()
                .map(|c| c.number.sensitivity)
                .unwrap_or_default(),
        },
        expires: exp,
    }))
}

impl Api {
    pub(crate) fn contractor_call(
        &mut self,
        cmd: &str,
        a: Value,
    ) -> Option<Result<Value, ApiError>> {
        Some(match cmd {
            "contractors" => self.contractors().and_then(ok),
            "contractor" => args(a).and_then(|a: IdArgs| ok(self.contractor_page(&a.id)?)),
            "save_contractor" => args(a).and_then(|a: ContractorIn| {
                self.save_contractor(a)?;
                ok(self.contractors()?)
            }),
            "delete_contractor" => args(a).and_then(|a: IdArgs| {
                if !self.me()?.role.can_edit_plan() {
                    return Err(ApiError::invalid(
                        "only household members can do this".to_string(),
                    ));
                }
                let id: ContractorId = parse_id(&a.id, "contractor")?;
                let _: Contractor = self.store.fetch(&id.to_string())?;
                // Their jobs stay in the history; they just lose the name.
                self.store.delete(EntityKind::Contractor, &id.to_string())?;
                ok(())
            }),
            _ => return None,
        })
    }

    fn contractor_out(
        &self,
        c: Contractor,
        jobs: &BTreeMap<ContractorId, Vec<contractors::Job>>,
        today: Date,
        me: &Member,
    ) -> ContractorOut {
        let see_sensitive = me.role.can_see_sensitive();
        let see_private = me.role.can_edit_plan();
        let cred = |x: Option<Credential>| {
            x.map(|x| {
                let state = match x.expires {
                    None => "no_date",
                    Some(e) if e < today => "ended",
                    Some(e)
                        if housewright_core::warranty::days_left(e, today)
                            <= CREDENTIAL_SOON_DAYS =>
                    {
                        "ending_soon"
                    }
                    Some(_) => "ok",
                };
                CredentialOut {
                    number: (x.number.sensitivity == Sensitivity::Normal || see_sensitive)
                        .then_some(x.number.value)
                        .filter(|n| !n.is_empty()),
                    expires: x.expires,
                    state,
                }
            })
        };
        let private = c
            .notes
            .as_ref()
            .is_some_and(|n| n.sensitivity == Sensitivity::Sensitive);
        let js = jobs.get(&c.id).map(Vec::as_slice).unwrap_or(&[]);
        ContractorOut {
            id: c.id.to_string(),
            name: c.name,
            company: c.company,
            trades: c.trades,
            phone: c.phone,
            email: c.email,
            website: c.website,
            service_area: c.service_area,
            notes: c
                .notes
                .filter(|_| !private || see_sensitive)
                .map(|n| n.value),
            notes_private: private,
            emergency: c.emergency,
            licence: cred(c.licence),
            insurance: cred(c.insurance),
            private: see_private.then_some(PrivateOut {
                rating: c.private_rating,
                would_hire_again: c.would_hire_again,
                notes: c.private_notes,
            }),
            jobs: js.len(),
            spent: js.iter().map(|j| j.total).sum(),
        }
    }

    fn contractors(&self) -> Result<Vec<ContractorOut>, ApiError> {
        let me = self.me()?;
        let p = self.first_property()?;
        let today = self.today_in(&p.time_zone)?;
        let jobs = contractors::jobs(&self.store)?;
        let mut v: Vec<Contractor> = self.store.list()?;
        v.sort_by_key(|c| c.name.to_lowercase());
        Ok(v.into_iter()
            .map(|c| self.contractor_out(c, &jobs, today, &me))
            .collect())
    }

    fn contractor_page(&self, id: &str) -> Result<ContractorPage, ApiError> {
        let id: ContractorId = parse_id(id, "contractor")?;
        let c: Contractor = self.store.fetch(&id.to_string())?;
        let me = self.me()?;
        let p = self.first_property()?;
        let today = self.today_in(&p.time_zone)?;
        let mut all = contractors::jobs(&self.store)?;
        let rooms: BTreeMap<RoomId, String> = self
            .store
            .list::<Room>()?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();
        let see = me.role.can_see_sensitive();
        let sensitive_items: Vec<ItemId> = self
            .store
            .list::<Item>()?
            .into_iter()
            .filter(|i| i.sensitivity == Sensitivity::Sensitive)
            .map(|i| i.id)
            .collect();
        let jobs = all
            .get(&id)
            .map(|js| {
                js.iter()
                    .map(|j| {
                        let hide = !see && j.item.is_some_and(|i| sensitive_items.contains(&i));
                        JobOut {
                            completion_id: j.completion.to_string(),
                            task_id: j.task.to_string(),
                            task_title: j.task_title.clone(),
                            at: j.at,
                            item_id: j.item.filter(|_| !hide).map(|i| i.to_string()),
                            item_name: if hide {
                                Some("Something marked private".into())
                            } else {
                                j.item_name.clone()
                            },
                            room_name: j
                                .room
                                .and_then(|r| rooms.get(&r).cloned())
                                .filter(|_| !hide),
                            costs: j
                                .lines
                                .iter()
                                .map(|l| CostLineOut {
                                    kind: l.kind,
                                    minor: l.amount.minor,
                                    currency: l.amount.currency.clone(),
                                })
                                .collect(),
                            total: j.total,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let attachments = self
            .store
            .attachments_about(&EntityRef {
                kind: EntityKind::Contractor,
                id: id.to_string(),
            })?
            .iter()
            .filter(|a| see || a.sensitivity == Sensitivity::Normal)
            .map(records::attachment_out)
            .collect();
        let currency = self.household()?.currency().to_string();
        let jobs_map = std::mem::take(&mut all);
        Ok(ContractorPage {
            contractor: self.contractor_out(c, &jobs_map, today, &me),
            jobs,
            currency,
            attachments,
        })
    }

    fn save_contractor(&mut self, a: ContractorIn) -> Result<ContractorId, ApiError> {
        let me = self.me()?;
        if !me.role.can_edit_plan() {
            return Err(ApiError::invalid(
                "only household members can do this".to_string(),
            ));
        }
        let name = a.name.trim().to_string();
        if name.is_empty() {
            return Err(ApiError::invalid("give it a name".to_string()));
        }
        let existing: Option<Contractor> = match &a.id {
            Some(id) => Some(
                self.store
                    .fetch(&parse_id::<ContractorId>(id, "contractor")?.to_string())?,
            ),
            None => None,
        };
        let old = |f: fn(&Contractor) -> Option<String>| existing.as_ref().and_then(f);
        let keep_or = |v: Option<String>, f: fn(&Contractor) -> Option<String>| match v {
            Some(v) => blank(Some(v)),
            None => old(f),
        };
        let notes = blank(a.notes).map(|n| Flagged {
            value: n,
            sensitivity: if a.notes_private {
                Sensitivity::Sensitive
            } else {
                Sensitivity::Normal
            },
        });
        let website = match a.website {
            Some(w) => website(&w)?,
            None => old(|c| c.website.clone()),
        };
        let rating = match a.private_rating {
            Some(0) => None,
            Some(r @ 1..=5) => Some(r),
            Some(_) => return Err(ApiError::invalid("a rating is 1 to 5".to_string())),
            None => existing.as_ref().and_then(|c| c.private_rating),
        };
        let again = match a.would_hire_again.as_deref() {
            Some("yes") => Some(true),
            Some("no") => Some(false),
            Some("") => None,
            Some(x) => return Err(ApiError::invalid(format!("'{x}' is not yes or no"))),
            None => existing.as_ref().and_then(|c| c.would_hire_again),
        };
        let c = Contractor {
            id: existing.as_ref().map(|c| c.id).unwrap_or_default(),
            name,
            company: keep_or(a.company, |c| c.company.clone()),
            trades: a
                .trades
                .into_iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect(),
            phone: blank(a.phone),
            email: blank(a.email),
            website,
            service_area: keep_or(a.service_area, |c| c.service_area.clone()),
            licence: credential(
                existing.as_ref().and_then(|c| c.licence.clone()),
                a.licence_number,
                a.licence_expires,
            )?,
            insurance: credential(
                existing.as_ref().and_then(|c| c.insurance.clone()),
                a.insurance_number,
                a.insurance_expires,
            )?,
            private_rating: rating,
            notes,
            emergency: a.emergency,
            would_hire_again: again,
            private_notes: keep_or(a.private_notes, |c| c.private_notes.clone()),
        };
        self.store.put(&c)?;
        Ok(c.id)
    }
}
