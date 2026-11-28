//! Prioritisation: what matters most (P3, P10).

use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::ids::{ItemId, TaskId};
use crate::schedule::{Evaluation, TaskState};

/// Priority classes, most important first. The derived order is the ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityClass {
    /// Smoke and CO alarms, gas, electrical hazards.
    LifeSafety,
    /// Water and fire damage to the property (leaks, dryer vents, sump pumps).
    PropertyLoss,
    /// Efficiency and lifespan (filters, coils, flushing).
    Efficiency,
    /// How it looks.
    Cosmetic,
}

impl PriorityClass {
    pub fn label(self) -> &'static str {
        match self {
            PriorityClass::LifeSafety => "life safety",
            PriorityClass::PropertyLoss => "property loss",
            PriorityClass::Efficiency => "efficiency/lifespan",
            PriorityClass::Cosmetic => "cosmetic",
        }
    }
}

/// A task with its evaluation, ready to rank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskStatus {
    pub task: TaskId,
    pub title: String,
    pub priority: PriorityClass,
    pub eval: Evaluation,
}

impl TaskStatus {
    fn due(&self) -> Option<Date> {
        self.eval.window.map(|w| w.due)
    }
}

/// Order: priority class, then urgency (overdue first), then the earlier due
/// date, then title and id so the order is total and stable.
pub fn rank(tasks: &mut [TaskStatus]) {
    tasks.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(b.eval.state.urgency().cmp(&a.eval.state.urgency()))
            .then(cmp_due(a.due(), b.due()))
            .then(a.title.cmp(&b.title))
            .then(a.task.cmp(&b.task))
    });
}

fn cmp_due(a: Option<Date>, b: Option<Date>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// How many tasks to switch on after onboarding (P3 says 3 to 5).
pub const START_SMALL_DEFAULT: usize = 5;

/// P3, start small: from the full suggested plan, the `n` tasks to switch on
/// first. Life safety first, then whatever is most urgent. Tasks that are
/// paused, not applicable or completed are never picked.
pub fn start_small(plan: &[TaskStatus], n: usize) -> Vec<TaskId> {
    let mut live: Vec<TaskStatus> = plan
        .iter()
        .filter(|t| t.eval.state.is_live())
        .cloned()
        .collect();
    rank(&mut live);
    live.into_iter().take(n).map(|t| t.task).collect()
}

/// One suggested task, as the starting-plan chooser sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct StartCandidate {
    pub status: TaskStatus,
    /// The thing it belongs to, so the first tasks cover different things.
    pub item: Option<ItemId>,
    /// Typical days between occurrences ([`crate::schedule::Schedule::period_days`]).
    pub period_days: u32,
    /// Order within a class, lower first (D-66): 0 for the smoke and CO
    /// alarms, which protect the people rather than the house; 1 for things
    /// where water, fire or gas do the damage (plumbing, appliances, heating,
    /// electrical); 2 for the outside and the structure.
    pub hint: u8,
}

/// Tasks that come round at most this often count as routine. Routine tasks
/// make better first tasks than once-a-decade replacements (D-66).
pub const ROUTINE_MAX_DAYS: u32 = 366;

/// P3 after onboarding: the `n` tasks to switch on first (D-66).
///
/// - Life safety first, then property loss, efficiency, cosmetic; within
///   a class, by `hint` (alarms, then water, fire and gas risks, then the
///   outside of the house).
/// - Within a class, routine tasks (yearly or more often) before long-cycle
///   ones, then the usual urgency and due-date order, then the more
///   frequent task (a monthly test before a yearly battery swap).
/// - At most one task per thing until every thing has had its turn, so the
///   plan is not five smoke-alarm jobs.
/// - One place is kept for the most important property-loss task (the
///   riskiest water or fire item, human-factors review section 7) when life
///   safety tasks would otherwise fill every place.
pub fn starting_plan(cands: &[StartCandidate], n: usize) -> Vec<TaskId> {
    let mut live: Vec<&StartCandidate> = cands
        .iter()
        .filter(|c| c.status.eval.state.is_live())
        .collect();
    live.sort_by(|a, b| {
        a.status
            .priority
            .cmp(&b.status.priority)
            .then(a.hint.cmp(&b.hint))
            .then((a.period_days > ROUTINE_MAX_DAYS).cmp(&(b.period_days > ROUTINE_MAX_DAYS)))
            .then(
                b.status
                    .eval
                    .state
                    .urgency()
                    .cmp(&a.status.eval.state.urgency()),
            )
            .then(cmp_due(a.status.due(), b.status.due()))
            .then(a.period_days.cmp(&b.period_days))
            .then(a.status.title.cmp(&b.status.title))
            .then(a.status.task.cmp(&b.status.task))
    });
    let has_loss = live
        .iter()
        .any(|c| c.status.priority == PriorityClass::PropertyLoss);
    let mut picked: Vec<TaskId> = Vec::new();
    let mut used_items: Vec<ItemId> = Vec::new();
    let mut have_loss = false;
    for c in &live {
        if picked.len() >= n {
            break;
        }
        if c.item.is_some_and(|i| used_items.contains(&i)) {
            continue;
        }
        let is_loss = c.status.priority == PriorityClass::PropertyLoss;
        if has_loss && !have_loss && !is_loss && picked.len() + 1 == n {
            continue;
        }
        have_loss |= is_loss;
        picked.push(c.status.task);
        used_items.extend(c.item);
    }
    for c in &live {
        if picked.len() >= n {
            break;
        }
        if !picked.contains(&c.status.task) {
            picked.push(c.status.task);
        }
    }
    picked
}

/// P10: after a lapse, show "these matter most", not the whole overdue pile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WelcomeBack {
    /// At most three overdue tasks, most important first.
    pub focus: Vec<TaskStatus>,
    /// How many other overdue tasks exist. The interface should not lead with
    /// this number (anti-pattern: "37 overdue").
    pub others_overdue: usize,
}

/// How many tasks the welcome-back view shows.
pub const WELCOME_BACK_COUNT: usize = 3;

/// Triage after a lapse: the top three overdue tasks by priority class, then
/// the longest overdue.
pub fn welcome_back(tasks: &[TaskStatus]) -> WelcomeBack {
    let mut overdue: Vec<TaskStatus> = tasks
        .iter()
        .filter(|t| t.eval.state == TaskState::Overdue)
        .cloned()
        .collect();
    rank(&mut overdue);
    let others = overdue.len().saturating_sub(WELCOME_BACK_COUNT);
    overdue.truncate(WELCOME_BACK_COUNT);
    WelcomeBack {
        focus: overdue,
        others_overdue: others,
    }
}

/// How long away counts as a lapse (P10, D-60).
pub const LAPSE_DAYS: i64 = 21;

/// P10: whether someone coming back should see the welcome-back view rather
/// than the plain list. True when they have been away at least
/// [`LAPSE_DAYS`] (or never opened the app on this device) and something is
/// overdue. Pure: `last_seen` and `now` are inputs.
pub fn is_lapsed(last_seen: Option<Timestamp>, now: Timestamp, tasks: &[TaskStatus]) -> bool {
    let away = match last_seen {
        Some(seen) => now.as_second() - seen.as_second() >= LAPSE_DAYS * 86_400,
        None => true,
    };
    away && tasks.iter().any(|t| t.eval.state == TaskState::Overdue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::DueWindow;
    use jiff::civil::date;

    fn st(title: &str, p: PriorityClass, state: TaskState, due: Option<Date>) -> TaskStatus {
        TaskStatus {
            task: TaskId::new(),
            title: title.into(),
            priority: p,
            eval: Evaluation {
                state,
                window: due.map(|d| DueWindow {
                    earliest: d,
                    due: d,
                    overdue_after: d,
                }),
                last_done: None,
                snooze_count: 0,
                needs_review: false,
                usage: None,
            },
        }
    }

    #[test]
    fn class_beats_urgency() {
        use PriorityClass::*;
        let mut v = vec![
            st(
                "paint",
                Cosmetic,
                TaskState::Overdue,
                Some(date(2026, 1, 1)),
            ),
            st("filter", Efficiency, TaskState::Due, Some(date(2026, 2, 1))),
            st(
                "smoke",
                LifeSafety,
                TaskState::Scheduled,
                Some(date(2026, 9, 1)),
            ),
            st(
                "dryer",
                PropertyLoss,
                TaskState::Overdue,
                Some(date(2026, 3, 1)),
            ),
        ];
        rank(&mut v);
        let titles: Vec<_> = v.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["smoke", "dryer", "filter", "paint"]);
    }

    #[test]
    fn urgency_then_due_within_class() {
        use PriorityClass::*;
        let mut v = vec![
            st(
                "b",
                Efficiency,
                TaskState::Scheduled,
                Some(date(2026, 1, 1)),
            ),
            st("c", Efficiency, TaskState::Overdue, Some(date(2026, 3, 1))),
            st("a", Efficiency, TaskState::Overdue, Some(date(2026, 2, 1))),
            st("d", Efficiency, TaskState::Scheduled, None),
        ];
        rank(&mut v);
        let titles: Vec<_> = v.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["a", "c", "b", "d"]);
    }

    #[test]
    fn start_small_takes_top_n_life_safety_first_and_skips_dead() {
        use PriorityClass::*;
        let plan = vec![
            st(
                "gutters",
                PropertyLoss,
                TaskState::Due,
                Some(date(2026, 1, 1)),
            ),
            st(
                "smoke",
                LifeSafety,
                TaskState::Scheduled,
                Some(date(2026, 6, 1)),
            ),
            st(
                "co",
                LifeSafety,
                TaskState::Scheduled,
                Some(date(2026, 5, 1)),
            ),
            st("na", LifeSafety, TaskState::NotApplicable, None),
            st("paint", Cosmetic, TaskState::Overdue, None),
            st("filter", Efficiency, TaskState::Due, None),
            st("coils", Efficiency, TaskState::Scheduled, None),
        ];
        let picked = start_small(&plan, START_SMALL_DEFAULT);
        assert_eq!(picked.len(), 5);
        let names: Vec<_> = picked
            .iter()
            .map(|id| plan.iter().find(|t| &t.task == id).unwrap().title.as_str())
            .collect();
        assert_eq!(names, ["co", "smoke", "gutters", "filter", "coils"]);
        assert!(start_small(&plan, 0).is_empty());
    }

    fn cand(
        title: &str,
        p: PriorityClass,
        item: Option<ItemId>,
        period_days: u32,
        state: TaskState,
    ) -> StartCandidate {
        StartCandidate {
            status: st(title, p, state, Some(date(2026, 12, 1))),
            item,
            period_days,
            hint: 1,
        }
    }

    fn titles(plan: &[StartCandidate], ids: &[TaskId]) -> Vec<String> {
        ids.iter()
            .map(|id| {
                plan.iter()
                    .find(|c| &c.status.task == id)
                    .unwrap()
                    .status
                    .title
                    .clone()
            })
            .collect()
    }

    #[test]
    fn starting_plan_life_safety_first_one_per_thing_routine_first() {
        use PriorityClass::*;
        let smoke = Some(ItemId::new());
        let co = Some(ItemId::new());
        let wh = Some(ItemId::new());
        let washer = Some(ItemId::new());
        let filter = Some(ItemId::new());
        let plan = vec![
            cand("smoke replace", LifeSafety, smoke, 3650, TaskState::Due),
            cand("smoke battery", LifeSafety, smoke, 365, TaskState::Due),
            cand("smoke test", LifeSafety, smoke, 30, TaskState::Due),
            cand("co test", LifeSafety, co, 30, TaskState::Due),
            cand("wh relief valve", LifeSafety, wh, 365, TaskState::Scheduled),
            cand(
                "washer hoses",
                PropertyLoss,
                washer,
                180,
                TaskState::Scheduled,
            ),
            cand("filter", Efficiency, filter, 90, TaskState::Overdue),
            cand("na", LifeSafety, None, 30, TaskState::NotApplicable),
        ];
        let three = titles(&plan, &starting_plan(&plan, 3));
        // Two life-safety things, then the place kept for water damage.
        assert_eq!(three, ["co test", "smoke test", "washer hoses"]);
        let five = titles(&plan, &starting_plan(&plan, 5));
        assert_eq!(
            five,
            [
                "co test",
                "smoke test",
                "wh relief valve",
                "washer hoses",
                "filter"
            ]
        );
        // Long-cycle replacements never come before the routine test.
        assert!(!five.contains(&"smoke replace".to_string()));
        // With more places than things, the second pass fills in order.
        let all = titles(&plan, &starting_plan(&plan, 7));
        assert_eq!(all.len(), 7);
        assert_eq!(all[5], "smoke battery");
    }

    #[test]
    fn starting_plan_without_property_loss_uses_every_place() {
        use PriorityClass::*;
        let plan = vec![
            cand("a", LifeSafety, Some(ItemId::new()), 30, TaskState::Due),
            cand("b", LifeSafety, Some(ItemId::new()), 30, TaskState::Due),
            cand("c", Efficiency, Some(ItemId::new()), 30, TaskState::Due),
        ];
        assert_eq!(titles(&plan, &starting_plan(&plan, 2)), ["a", "b"]);
        assert!(starting_plan(&[], 4).is_empty());
    }

    #[test]
    fn welcome_back_top_three_overdue() {
        use PriorityClass::*;
        let mut v = Vec::new();
        for i in 0..10 {
            v.push(st(
                &format!("cos{i}"),
                Cosmetic,
                TaskState::Overdue,
                Some(date(2026, 1, 1)),
            ));
        }
        v.push(st(
            "co",
            LifeSafety,
            TaskState::Overdue,
            Some(date(2026, 3, 1)),
        ));
        v.push(st(
            "smoke",
            LifeSafety,
            TaskState::Due,
            Some(date(2026, 3, 1)),
        ));
        v.push(st(
            "leak",
            PropertyLoss,
            TaskState::Overdue,
            Some(date(2026, 2, 1)),
        ));
        let wb = welcome_back(&v);
        let names: Vec<_> = wb.focus.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(names, ["co", "leak", "cos0"]);
        assert_eq!(wb.others_overdue, 9);
        assert!(welcome_back(&[]).focus.is_empty());
    }

    #[test]
    fn lapse_needs_time_away_and_something_overdue() {
        let now = Timestamp::from_second(1_795_780_800).unwrap();
        let day = 86_400;
        let recent = Timestamp::from_second(now.as_second() - 3 * day).unwrap();
        let long_ago = Timestamp::from_second(now.as_second() - 40 * day).unwrap();
        let overdue = vec![st("a", PriorityClass::Efficiency, TaskState::Overdue, None)];
        let fine = vec![st("a", PriorityClass::Efficiency, TaskState::Due, None)];
        assert!(is_lapsed(Some(long_ago), now, &overdue));
        assert!(!is_lapsed(Some(recent), now, &overdue));
        assert!(!is_lapsed(Some(long_ago), now, &fine));
        assert!(is_lapsed(None, now, &overdue));
    }
}
