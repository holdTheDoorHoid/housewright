//! Prioritisation: what matters most (P3, P10).

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::ids::TaskId;
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
}
