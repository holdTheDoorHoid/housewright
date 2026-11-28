//! Contractors' job history (D-28): which completions a contractor did,
//! what they cost, and where. Pure over the store's records.
//!
//! A completion is linked to a contractor after Done (P1) by a
//! [`CompletionDetail`]: the latest detail that names a contractor, or
//! says "not a contractor after all", wins; otherwise the completion's own
//! `contractor` field (another program, older data) counts.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::Serialize;

use crate::costs::{effective, effective_costs};
use crate::error::Result;
use crate::ids::{CompletionId, ContractorId, ItemId, RoomId, TaskId};
use crate::model::{Completion, CompletionDetail, CostLine, Item, Task};
use crate::store::Store;

/// Who did a completion, as far as the household has said.
pub fn effective_contractor(c: &Completion, details: &[CompletionDetail]) -> Option<ContractorId> {
    let latest = details
        .iter()
        .filter(|d| d.completion == c.id && (d.contractor.is_some() || d.contractor_cleared))
        .max_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    match latest {
        Some(d) if d.contractor_cleared => None,
        Some(d) => d.contractor,
        None => c.contractor,
    }
}

/// One job a contractor did.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Job {
    pub completion: CompletionId,
    pub task: TaskId,
    pub task_title: String,
    pub at: Timestamp,
    pub item: Option<ItemId>,
    pub item_name: Option<String>,
    pub room: Option<RoomId>,
    pub lines: Vec<CostLine>,
    /// The cost in the household's currency, in minor units.
    pub total: i64,
}

/// Every job, newest first, grouped by contractor. Voided completions do
/// not count.
pub fn jobs(store: &Store) -> Result<BTreeMap<ContractorId, Vec<Job>>> {
    let currency = store.household()?.currency().to_string();
    let tasks: BTreeMap<TaskId, Task> = store
        .list::<Task>()?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    let items: BTreeMap<ItemId, Item> = store
        .list::<Item>()?
        .into_iter()
        .map(|i| (i.id, i))
        .collect();
    let mut details: BTreeMap<CompletionId, Vec<CompletionDetail>> = BTreeMap::new();
    for d in store.list::<CompletionDetail>()? {
        details.entry(d.completion).or_default().push(d);
    }
    let mut out: BTreeMap<ContractorId, Vec<Job>> = BTreeMap::new();
    for c in effective(store.list::<Completion>()?) {
        let ds = details.get(&c.id).map(Vec::as_slice).unwrap_or(&[]);
        let Some(k) = effective_contractor(&c, ds) else {
            continue;
        };
        let Some(t) = tasks.get(&c.task) else {
            continue;
        };
        let lines = effective_costs(&c, ds);
        let total = lines
            .iter()
            .filter(|l| l.amount.currency == currency)
            .map(|l| l.amount.minor)
            .sum();
        let item = t.item.and_then(|i| items.get(&i));
        out.entry(k).or_default().push(Job {
            completion: c.id,
            task: t.id,
            task_title: t.title.clone(),
            at: c.at,
            item: t.item,
            item_name: item.map(|i| i.name.clone()),
            room: item.and_then(|i| i.room).or(t.room),
            lines,
            total,
        });
    }
    for v in out.values_mut() {
        v.sort_by_key(|j| std::cmp::Reverse(j.at));
    }
    Ok(out)
}
