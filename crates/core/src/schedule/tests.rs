use super::*;
use jiff::civil::date;

fn ny() -> TimeZone {
    TimeZone::get("America/New_York").expect("tz database")
}

/// Noon local time on a date, as an instant.
fn at(d: Date, tz: &TimeZone) -> Timestamp {
    d.at(12, 0, 0, 0)
        .to_zoned_datetime(tz.clone())
        .unwrap()
        .timestamp()
}

struct Case {
    schedule: Schedule,
    window: Option<WindowPolicy>,
    start: Date,
    done: Vec<Date>,
    snoozes: Vec<(Date, Date)>,
    readings: Vec<(Date, f64)>,
    climate: Climate,
    paused: bool,
    na: bool,
}

impl Case {
    fn new(schedule: Schedule, start: Date) -> Self {
        Self {
            schedule,
            window: None,
            start,
            done: vec![],
            snoozes: vec![],
            readings: vec![],
            climate: Climate::default(),
            paused: false,
            na: false,
        }
    }
    fn done(mut self, d: Date) -> Self {
        self.done.push(d);
        self
    }
    fn window(mut self, lead: u32, grace: u32) -> Self {
        self.window = Some(WindowPolicy {
            lead_days: lead,
            grace_days: grace,
        });
        self
    }
    fn eval(&self, today: Date) -> Evaluation {
        let tz = ny();
        let completions: Vec<Timestamp> = self.done.iter().map(|d| at(*d, &tz)).collect();
        // Snoozes are recorded at 13:00 so they sort after a same-day completion.
        let snoozes: Vec<SnoozeInput> = self
            .snoozes
            .iter()
            .map(|(on, until)| SnoozeInput {
                at: on
                    .at(13, 0, 0, 0)
                    .to_zoned_datetime(tz.clone())
                    .unwrap()
                    .timestamp(),
                until: *until,
            })
            .collect();
        let readings: Vec<ReadingInput> = self
            .readings
            .iter()
            .map(|(d, v)| ReadingInput {
                at: at(*d, &tz),
                value: *v,
            })
            .collect();
        let input = ScheduleInput {
            schedule: &self.schedule,
            window: self.window,
            start: self.start,
            completions: &completions,
            snoozes: &snoozes,
            readings: &readings,
            climate: &self.climate,
            paused: self.paused,
            not_applicable: self.na,
        };
        evaluate(&input, at(today, &tz), &tz)
    }
}

fn due(e: &Evaluation) -> Date {
    e.window.expect("window").due
}

fn monthly(anchor: Date) -> Schedule {
    Schedule::Fixed {
        every: Interval::months(1),
        anchor: Some(anchor),
    }
}

// --- intervals ------------------------------------------------------------

#[test]
fn interval_parse_and_display() {
    assert_eq!("3 months".parse::<Interval>().unwrap(), Interval::months(3));
    assert_eq!("1 year".parse::<Interval>().unwrap(), Interval::years(1));
    assert_eq!("90 Days".parse::<Interval>().unwrap(), Interval::days(90));
    assert_eq!("2 weeks".parse::<Interval>().unwrap(), Interval::weeks(2));
    assert!("0 days".parse::<Interval>().is_err());
    assert!("monthly".parse::<Interval>().is_err());
    assert!("3 fortnights".parse::<Interval>().is_err());
    assert!("-1 days".parse::<Interval>().is_err());
    assert_eq!(Interval::months(1).to_string(), "1 month");
    assert_eq!(Interval::days(90).to_string(), "90 days");
}

#[test]
fn month_end_clamping() {
    let m = Interval::months(1);
    assert_eq!(m.add_to(date(2026, 1, 31)), Some(date(2026, 2, 28)));
    assert_eq!(m.add_to(date(2028, 1, 31)), Some(date(2028, 2, 29)));
    assert_eq!(m.add_to(date(2026, 3, 31)), Some(date(2026, 4, 30)));
    // Computed from the anchor, occurrences do not drift down to the 28th.
    assert_eq!(m.add_times(date(2026, 1, 31), 2), Some(date(2026, 3, 31)));
    assert_eq!(m.add_times(date(2026, 1, 31), 3), Some(date(2026, 4, 30)));
}

#[test]
fn leap_years() {
    let y = Interval::years(1);
    assert_eq!(y.add_to(date(2028, 2, 29)), Some(date(2029, 2, 28)));
    assert_eq!(y.add_times(date(2028, 2, 29), 4), Some(date(2032, 2, 29)));
    assert_eq!(
        Interval::days(1).add_to(date(2028, 2, 28)),
        Some(date(2028, 2, 29))
    );
    assert_eq!(
        Interval::days(1).add_to(date(2027, 2, 28)),
        Some(date(2027, 3, 1))
    );
}

#[test]
fn default_windows() {
    assert_eq!(
        WindowPolicy::for_period(30),
        WindowPolicy {
            lead_days: 5,
            grace_days: 5
        }
    );
    assert_eq!(
        WindowPolicy::for_period(365),
        WindowPolicy {
            lead_days: 14,
            grace_days: 30
        }
    );
    assert_eq!(
        WindowPolicy::for_period(1),
        WindowPolicy {
            lead_days: 0,
            grace_days: 0
        }
    );
}

// --- fixed ----------------------------------------------------------------

#[test]
fn fixed_states_across_window() {
    // Monthly on the 1st, window 5/5.
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1));
    let e = c.eval(date(2026, 1, 1));
    assert_eq!(e.state, TaskState::Due);
    assert_eq!(
        e.window,
        Some(DueWindow {
            earliest: date(2025, 12, 27),
            due: date(2026, 1, 1),
            overdue_after: date(2026, 1, 6)
        })
    );
    assert_eq!(c.eval(date(2026, 1, 6)).state, TaskState::Due);
    assert_eq!(c.eval(date(2026, 1, 7)).state, TaskState::Overdue);
    // Done on the 3rd: next is Feb 1.
    let c = c.done(date(2026, 1, 3));
    let e = c.eval(date(2026, 1, 10));
    assert_eq!(e.state, TaskState::Scheduled);
    assert_eq!(due(&e), date(2026, 2, 1));
    assert_eq!(c.eval(date(2026, 1, 27)).state, TaskState::DueSoon);
    assert_eq!(c.eval(date(2026, 2, 1)).state, TaskState::Due);
}

#[test]
fn fixed_is_independent_of_completion_time() {
    // Done late, on the 20th: still due on the 1st of next month.
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1)).done(date(2026, 1, 20));
    assert_eq!(due(&c.eval(date(2026, 1, 21))), date(2026, 2, 1));
}

#[test]
fn fixed_early_completion_inside_lead_counts() {
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1)).done(date(2026, 1, 28));
    // Feb 1 window opens Jan 27; doing it on Jan 28 satisfies it.
    assert_eq!(due(&c.eval(date(2026, 1, 29))), date(2026, 3, 1));
}

#[test]
fn fixed_missed_occurrences_collapse_to_first_unsatisfied() {
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1)).done(date(2026, 1, 2));
    let e = c.eval(date(2026, 6, 15));
    assert_eq!(e.state, TaskState::Overdue);
    assert_eq!(due(&e), date(2026, 2, 1));
    // One completion catches up all of them.
    let c = c.done(date(2026, 6, 15));
    assert_eq!(due(&c.eval(date(2026, 6, 16))), date(2026, 7, 1));
}

#[test]
fn fixed_month_end_anchor_does_not_drift() {
    let c = Case::new(monthly(date(2026, 1, 31)), date(2026, 1, 1)).done(date(2026, 2, 27));
    // Feb occurrence is Feb 28 (clamped); done inside its lead.
    assert_eq!(due(&c.eval(date(2026, 3, 1))), date(2026, 3, 31));
    let c = Case::new(monthly(date(2026, 1, 31)), date(2026, 4, 6));
    assert_eq!(due(&c.eval(date(2026, 4, 6))), date(2026, 4, 30));
}

#[test]
fn fixed_without_anchor_counts_from_start() {
    let c = Case::new(
        Schedule::Fixed {
            every: Interval::months(3),
            anchor: None,
        },
        date(2026, 2, 10),
    );
    assert_eq!(due(&c.eval(date(2026, 2, 10))), date(2026, 2, 10));
    let c = c.done(date(2026, 2, 10));
    assert_eq!(due(&c.eval(date(2026, 2, 11))), date(2026, 5, 10));
}

#[test]
fn fixed_old_anchor_new_task_starts_at_current_occurrence() {
    // Anchor years back; the task was added today with no history.
    let c = Case::new(monthly(date(2015, 3, 1)), date(2026, 11, 27));
    let e = c.eval(date(2026, 11, 27));
    assert_eq!(due(&e), date(2026, 12, 1));
    assert_eq!(e.state, TaskState::DueSoon);
}

#[test]
fn new_task_created_inside_an_open_window_owes_that_window() {
    // Monthly on the 1st with 5 days' grace; created on Jan 3 with no history:
    // the Jan 1 occurrence is still open, so it is due now, not on Feb 1.
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 3));
    let e = c.eval(date(2026, 1, 3));
    assert_eq!(due(&e), date(2026, 1, 1));
    assert_eq!(e.state, TaskState::Due);
    // Created after the grace period: the next occurrence.
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 7));
    assert_eq!(due(&c.eval(date(2026, 1, 7))), date(2026, 2, 1));
}

#[test]
fn fixed_days_weeks_years() {
    let c = Case::new(
        Schedule::Fixed {
            every: Interval::weeks(2),
            anchor: Some(date(2026, 1, 5)),
        },
        date(2026, 1, 1),
    )
    .done(date(2026, 1, 5));
    assert_eq!(due(&c.eval(date(2026, 1, 6))), date(2026, 1, 19));

    let c = Case::new(
        Schedule::Fixed {
            every: Interval::days(10),
            anchor: Some(date(2020, 1, 1)),
        },
        date(2026, 1, 1),
    );
    // 2026-01-01 is day 2192 after 2020-01-01; next multiple of 10 is 2200.
    assert_eq!(due(&c.eval(date(2026, 1, 1))), date(2026, 1, 9));

    let c = Case::new(
        Schedule::Fixed {
            every: Interval::years(1),
            anchor: Some(date(2024, 2, 29)),
        },
        date(2025, 1, 1),
    );
    assert_eq!(due(&c.eval(date(2025, 1, 1))), date(2025, 2, 28));
}

#[test]
fn daily_task_over_long_history_is_fast_and_right() {
    let c = Case::new(
        Schedule::Fixed {
            every: Interval::days(1),
            anchor: Some(date(1950, 1, 1)),
        },
        date(1950, 1, 1),
    )
    .done(date(2026, 11, 26));
    let e = c.eval(date(2026, 11, 27));
    assert_eq!(due(&e), date(2026, 11, 27));
    assert_eq!(e.state, TaskState::Due);
}

#[test]
fn nth_weekday() {
    // Second Saturday of every month.
    let s = Schedule::NthWeekday {
        nth: 2,
        weekday: WeekdayName::Saturday,
        months: vec![],
    };
    let c = Case::new(s, date(2026, 11, 1));
    assert_eq!(due(&c.eval(date(2026, 11, 1))), date(2026, 11, 14));
    let c = c.done(date(2026, 11, 14));
    assert_eq!(due(&c.eval(date(2026, 11, 15))), date(2026, 12, 12));

    // Last Sunday of March and October (clock-change cue, P4).
    let s = Schedule::NthWeekday {
        nth: -1,
        weekday: WeekdayName::Sunday,
        months: vec![3, 10],
    };
    let c = Case::new(s, date(2026, 5, 1));
    assert_eq!(due(&c.eval(date(2026, 5, 1))), date(2026, 10, 25));
    let c = c.done(date(2026, 10, 25));
    assert_eq!(due(&c.eval(date(2026, 10, 26))), date(2027, 3, 28));
}

#[test]
fn months_of_year() {
    let s = Schedule::MonthsOfYear {
        months: vec![4, 10],
        day: 1,
    };
    let c = Case::new(s, date(2026, 5, 2));
    assert_eq!(due(&c.eval(date(2026, 5, 2))), date(2026, 10, 1));
    let c = c.done(date(2026, 10, 2));
    assert_eq!(due(&c.eval(date(2026, 10, 3))), date(2027, 4, 1));

    // Day 31 in February clamps to the last day, leap year included.
    let s = Schedule::MonthsOfYear {
        months: vec![2],
        day: 31,
    };
    let c = Case::new(s, date(2028, 1, 1));
    assert_eq!(due(&c.eval(date(2028, 1, 1))), date(2028, 2, 29));
    let c = c.done(date(2028, 2, 29));
    assert_eq!(due(&c.eval(date(2028, 3, 1))), date(2029, 2, 28));
}

// --- floating -------------------------------------------------------------

#[test]
fn floating() {
    let s = Schedule::Floating {
        every: Interval::days(90),
    };
    let c = Case::new(s, date(2026, 1, 1));
    // Never done: due at start.
    assert_eq!(c.eval(date(2026, 1, 1)).state, TaskState::Due);
    let c = c.done(date(2026, 1, 20));
    let e = c.eval(date(2026, 1, 21));
    assert_eq!(due(&e), date(2026, 4, 20));
    assert_eq!(e.state, TaskState::Scheduled);
    // Window 14/15 for 90 days.
    assert_eq!(c.eval(date(2026, 4, 6)).state, TaskState::DueSoon);
    assert_eq!(c.eval(date(2026, 5, 5)).state, TaskState::Due);
    assert_eq!(c.eval(date(2026, 5, 6)).state, TaskState::Overdue);
    // Uses the latest completion, regardless of input order.
    let mut c = c.done(date(2026, 3, 1));
    c.done.reverse();
    assert_eq!(due(&c.eval(date(2026, 3, 2))), date(2026, 5, 30));
}

#[test]
fn floating_months_from_month_end() {
    let c = Case::new(
        Schedule::Floating {
            every: Interval::months(1),
        },
        date(2026, 1, 1),
    )
    .done(date(2026, 1, 31));
    assert_eq!(due(&c.eval(date(2026, 2, 1))), date(2026, 2, 28));
}

// --- DST and time zones -----------------------------------------------------

#[test]
fn dst_does_not_shift_dates() {
    // 2026-03-08 is the US spring-forward day; 2026-11-01 falls back.
    let tz = ny();
    let s = Schedule::Floating {
        every: Interval::days(1),
    };
    let late = date(2026, 3, 7)
        .at(23, 30, 0, 0)
        .to_zoned_datetime(tz.clone())
        .unwrap()
        .timestamp();
    let input = ScheduleInput {
        schedule: &s,
        window: None,
        start: date(2026, 1, 1),
        completions: &[late],
        snoozes: &[],
        readings: &[],
        climate: &Climate::default(),
        paused: false,
        not_applicable: false,
    };
    // One hour after midnight on the short day.
    let now = date(2026, 3, 8)
        .at(1, 0, 0, 0)
        .to_zoned_datetime(tz.clone())
        .unwrap()
        .timestamp();
    let e = evaluate(&input, now, &tz);
    assert_eq!(e.last_done, Some(date(2026, 3, 7)));
    assert_eq!(due(&e), date(2026, 3, 8));
    assert_eq!(e.state, TaskState::Due);

    // Fall back: 23:30 on Oct 31 + 1 day is Nov 1 regardless of the 25h day.
    let late = date(2026, 10, 31)
        .at(23, 30, 0, 0)
        .to_zoned_datetime(tz.clone())
        .unwrap()
        .timestamp();
    let input = ScheduleInput {
        completions: &[late],
        ..input
    };
    let now = date(2026, 11, 1)
        .at(23, 59, 0, 0)
        .to_zoned_datetime(tz.clone())
        .unwrap()
        .timestamp();
    let e = evaluate(&input, now, &tz);
    assert_eq!(due(&e), date(2026, 11, 1));
}

#[test]
fn same_instant_differs_by_time_zone() {
    // 03:00 UTC on Jan 2 is still Jan 1 in New York.
    let s = Schedule::OneOff {
        due: date(2026, 1, 2),
    };
    let input = ScheduleInput {
        schedule: &s,
        window: Some(WindowPolicy {
            lead_days: 0,
            grace_days: 0,
        }),
        start: date(2026, 1, 1),
        completions: &[],
        snoozes: &[],
        readings: &[],
        climate: &Climate::default(),
        paused: false,
        not_applicable: false,
    };
    let now: Timestamp = "2026-01-02T03:00:00Z".parse().unwrap();
    assert_eq!(evaluate(&input, now, &TimeZone::UTC).state, TaskState::Due);
    assert_eq!(evaluate(&input, now, &ny()).state, TaskState::Scheduled);
}

// --- one-off ----------------------------------------------------------------

#[test]
fn one_off() {
    let c = Case::new(
        Schedule::OneOff {
            due: date(2026, 12, 15),
        },
        date(2026, 11, 1),
    );
    assert_eq!(c.eval(date(2026, 11, 1)).state, TaskState::Scheduled);
    assert_eq!(c.eval(date(2026, 12, 8)).state, TaskState::DueSoon);
    assert_eq!(c.eval(date(2026, 12, 15)).state, TaskState::Due);
    assert_eq!(c.eval(date(2026, 12, 23)).state, TaskState::Overdue);
    let c = c.done(date(2026, 12, 1));
    assert_eq!(c.eval(date(2027, 6, 1)).state, TaskState::Completed);
}

// --- seasonal ---------------------------------------------------------------

#[test]
fn seasonal_fallback_north() {
    let c = Case::new(
        Schedule::Seasonal {
            season: Season::BeforeFirstFrost,
        },
        date(2026, 6, 1),
    );
    let e = c.eval(date(2026, 6, 1));
    assert_eq!(
        e.window,
        Some(DueWindow {
            earliest: date(2026, 8, 18),
            due: date(2026, 9, 1),
            overdue_after: date(2026, 10, 31)
        })
    );
    assert_eq!(e.state, TaskState::Scheduled);
    assert_eq!(c.eval(date(2026, 8, 20)).state, TaskState::DueSoon);
    assert_eq!(c.eval(date(2026, 10, 31)).state, TaskState::Due);
    assert_eq!(c.eval(date(2026, 11, 1)).state, TaskState::Overdue);
    let c = c.done(date(2026, 10, 10));
    assert_eq!(due(&c.eval(date(2026, 10, 11))), date(2027, 9, 1));
}

#[test]
fn seasonal_frost_date_from_property() {
    let mut c = Case::new(
        Schedule::Seasonal {
            season: Season::BeforeFirstFrost,
        },
        date(2026, 1, 1),
    );
    // Day 288 of a non-leap year is Oct 15.
    c.climate.first_frost_doy = Some(288);
    let e = c.eval(date(2026, 1, 1));
    let w = e.window.unwrap();
    assert_eq!(w.due, date(2026, 9, 3)); // Oct 15 - 42 days
    assert_eq!(w.overdue_after, date(2026, 10, 14));
    // Same day-of-year means the same calendar date in a leap year.
    let e = c.eval(date(2028, 1, 1));
    // Not done in 2026, so 2026's window is still owed...
    assert_eq!(e.state, TaskState::Overdue);
    let c = c.done(date(2027, 12, 31));
    assert_eq!(
        c.eval(date(2028, 1, 1)).window.unwrap().overdue_after,
        date(2028, 10, 14)
    );
}

#[test]
fn seasonal_after_last_frost() {
    let mut c = Case::new(
        Schedule::Seasonal {
            season: Season::AfterLastFrost,
        },
        date(2026, 1, 1),
    );
    c.climate.last_frost_doy = Some(105); // Apr 15
    let w = c.eval(date(2026, 1, 1)).window.unwrap();
    assert_eq!(w.due, date(2026, 4, 16));
    assert_eq!(w.overdue_after, date(2026, 5, 27));
}

#[test]
fn seasonal_southern_hemisphere() {
    let mut c = Case::new(
        Schedule::Seasonal {
            season: Season::Spring,
        },
        date(2026, 1, 1),
    );
    c.climate.hemisphere = Hemisphere::South;
    let w = c.eval(date(2026, 1, 1)).window.unwrap();
    assert_eq!(w.due, date(2026, 9, 1));
    assert_eq!(w.overdue_after, date(2026, 11, 30));

    let mut c = Case::new(
        Schedule::Seasonal {
            season: Season::BeforeFirstFrost,
        },
        date(2026, 1, 1),
    );
    c.climate.hemisphere = Hemisphere::South;
    let w = c.eval(date(2026, 1, 1)).window.unwrap();
    assert_eq!(
        (w.due, w.overdue_after),
        (date(2026, 3, 1), date(2026, 4, 30))
    );

    // Southern summer crosses the new year: Dec 1 - end of Feb.
    let mut c = Case::new(
        Schedule::Seasonal {
            season: Season::Summer,
        },
        date(2026, 1, 10),
    );
    c.climate.hemisphere = Hemisphere::South;
    let e = c.eval(date(2026, 1, 10));
    let w = e.window.unwrap();
    assert_eq!(
        (w.due, w.overdue_after),
        (date(2025, 12, 1), date(2026, 2, 28))
    );
    assert_eq!(e.state, TaskState::Due);
}

#[test]
fn northern_winter_crosses_year_and_leap_feb() {
    let c = Case::new(
        Schedule::Seasonal {
            season: Season::Winter,
        },
        date(2027, 6, 1),
    );
    let w = c.eval(date(2027, 6, 1)).window.unwrap();
    assert_eq!(
        (w.due, w.overdue_after),
        (date(2027, 12, 1), date(2028, 2, 29))
    );
}

#[test]
fn season_aliases_parse() {
    let s: Schedule = serde_json::from_str(r#"{"kind":"seasonal","season":"fall"}"#).unwrap();
    assert_eq!(
        s,
        Schedule::Seasonal {
            season: Season::Autumn
        }
    );
}

// --- usage ------------------------------------------------------------------

fn usage(or_every: Option<Interval>) -> Schedule {
    Schedule::Usage {
        meter: "runtime_hours".into(),
        threshold: 300.0,
        or_every,
    }
}

#[test]
fn usage_without_readings_has_no_date() {
    let c = Case::new(usage(None), date(2026, 1, 1));
    let e = c.eval(date(2026, 6, 1));
    assert_eq!(e.state, TaskState::Scheduled);
    assert_eq!(e.window, None);
    assert_eq!(e.usage, None);
}

#[test]
fn usage_projects_and_crosses() {
    let mut c = Case::new(usage(None), date(2026, 1, 1)).done(date(2026, 1, 1));
    c.readings = vec![(date(2026, 1, 1), 1000.0), (date(2026, 1, 31), 1100.0)];
    let e = c.eval(date(2026, 1, 31));
    assert_eq!(e.usage.unwrap().used, 100.0);
    // 100 hours per 30 days: 200 more takes 60 days.
    assert_eq!(due(&e), date(2026, 4, 1));
    assert_eq!(e.state, TaskState::Scheduled);

    c.readings.push((date(2026, 2, 25), 1275.0)); // 91.7%
    assert_eq!(c.eval(date(2026, 2, 25)).state, TaskState::DueSoon);

    c.readings.push((date(2026, 3, 5), 1310.0)); // crossed
    let e = c.eval(date(2026, 3, 6));
    assert_eq!(e.state, TaskState::Due);
    assert_eq!(due(&e), date(2026, 3, 5));
    // Grace 14 days by default.
    assert_eq!(c.eval(date(2026, 3, 20)).state, TaskState::Overdue);

    // Completion resets the baseline to the reading on/before that day.
    let c = c.done(date(2026, 3, 6));
    let e = c.eval(date(2026, 3, 7));
    assert_eq!(e.usage.unwrap().used, 0.0);
    assert!(e.state.urgency() <= TaskState::DueSoon.urgency());
}

#[test]
fn usage_or_calendar_whichever_first() {
    let mut c =
        Case::new(usage(Some(Interval::months(6))), date(2026, 1, 1)).done(date(2026, 1, 1));
    c.readings = vec![(date(2026, 1, 1), 0.0), (date(2026, 7, 1), 50.0)];
    // Barely used, but six months have passed.
    let e = c.eval(date(2026, 7, 2));
    assert_eq!(e.state, TaskState::Due);
    assert_eq!(due(&e), date(2026, 7, 1));

    // Heavy use crosses first.
    let mut c =
        Case::new(usage(Some(Interval::months(6))), date(2026, 1, 1)).done(date(2026, 1, 1));
    c.readings = vec![(date(2026, 1, 1), 0.0), (date(2026, 2, 1), 400.0)];
    let e = c.eval(date(2026, 2, 2));
    assert_eq!(e.state, TaskState::Due);
    assert_eq!(due(&e), date(2026, 2, 1));
}

#[test]
fn usage_meter_reset_does_not_go_negative() {
    let mut c = Case::new(usage(None), date(2026, 1, 1)).done(date(2026, 1, 1));
    c.readings = vec![(date(2026, 1, 1), 500.0), (date(2026, 2, 1), 10.0)];
    assert_eq!(c.eval(date(2026, 2, 2)).usage.unwrap().used, 0.0);
}

// --- overrides: snooze, pause, not applicable --------------------------------

#[test]
fn snooze_then_due_then_review_after_three() {
    let mut c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1));
    // Due Jan 1; snoozed on Jan 2 until Jan 10.
    c.snoozes.push((date(2026, 1, 2), date(2026, 1, 10)));
    let e = c.eval(date(2026, 1, 3));
    assert_eq!(e.state, TaskState::Snoozed);
    assert_eq!(due(&e), date(2026, 1, 10));
    assert_eq!(e.snooze_count, 1);
    assert!(!e.needs_review);
    assert_eq!(c.eval(date(2026, 1, 10)).state, TaskState::Due);
    // Grace for monthly is 5 days.
    assert_eq!(c.eval(date(2026, 1, 16)).state, TaskState::Overdue);

    c.snoozes.push((date(2026, 1, 10), date(2026, 1, 12)));
    c.snoozes.push((date(2026, 1, 12), date(2026, 1, 14)));
    let e = c.eval(date(2026, 1, 13));
    assert_eq!(e.snooze_count, 3);
    assert!(e.needs_review);
    assert_eq!(due(&e), date(2026, 1, 14));

    // Completing clears the snooze count.
    let c = c.done(date(2026, 1, 14));
    let e = c.eval(date(2026, 1, 15));
    assert_eq!(e.snooze_count, 0);
    assert!(!e.needs_review);
    assert_eq!(due(&e), date(2026, 2, 1));
}

#[test]
fn snooze_to_before_due_changes_nothing_but_counts() {
    let mut c = Case::new(
        Schedule::OneOff {
            due: date(2026, 2, 1),
        },
        date(2026, 1, 1),
    );
    c.snoozes.push((date(2026, 1, 5), date(2026, 1, 20)));
    let e = c.eval(date(2026, 1, 6));
    assert_eq!(due(&e), date(2026, 2, 1));
    assert_eq!(e.state, TaskState::Scheduled);
    assert_eq!(e.snooze_count, 1);
}

#[test]
fn reschedule_to_a_date_uses_latest_snooze() {
    let mut c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1));
    c.snoozes.push((date(2026, 1, 2), date(2026, 1, 20)));
    c.snoozes.push((date(2026, 1, 3), date(2026, 1, 8)));
    assert_eq!(due(&c.eval(date(2026, 1, 4))), date(2026, 1, 8));
}

#[test]
fn paused_and_not_applicable_override() {
    let mut c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1));
    c.paused = true;
    let e = c.eval(date(2026, 3, 1));
    assert_eq!(e.state, TaskState::Paused);
    assert_eq!(e.window, None);
    c.na = true;
    assert_eq!(c.eval(date(2026, 3, 1)).state, TaskState::NotApplicable);
}

#[test]
fn custom_window_override() {
    let c = Case::new(monthly(date(2026, 1, 1)), date(2026, 1, 1)).window(0, 0);
    assert_eq!(c.eval(date(2025, 12, 31)).state, TaskState::Scheduled);
    assert_eq!(c.eval(date(2026, 1, 1)).state, TaskState::Due);
    assert_eq!(c.eval(date(2026, 1, 2)).state, TaskState::Overdue);
}

// --- validation and serialisation ------------------------------------------

#[test]
fn validate_rejects_bad_parameters() {
    let bad = [
        Schedule::NthWeekday {
            nth: 5,
            weekday: WeekdayName::Monday,
            months: vec![],
        },
        Schedule::NthWeekday {
            nth: 0,
            weekday: WeekdayName::Monday,
            months: vec![],
        },
        Schedule::MonthsOfYear {
            months: vec![13],
            day: 1,
        },
        Schedule::MonthsOfYear {
            months: vec![],
            day: 1,
        },
        Schedule::MonthsOfYear {
            months: vec![1],
            day: 0,
        },
        Schedule::Usage {
            meter: "x".into(),
            threshold: 0.0,
            or_every: None,
        },
        Schedule::Usage {
            meter: "".into(),
            threshold: 1.0,
            or_every: None,
        },
    ];
    for s in bad {
        assert!(s.validate().is_err(), "{s:?}");
    }
}

#[test]
fn schedule_json_round_trip() {
    let all = vec![
        monthly(date(2026, 1, 31)),
        Schedule::NthWeekday {
            nth: -1,
            weekday: WeekdayName::Sunday,
            months: vec![3],
        },
        Schedule::MonthsOfYear {
            months: vec![10],
            day: 1,
        },
        Schedule::Floating {
            every: Interval::days(90),
        },
        Schedule::Seasonal {
            season: Season::BeforeFirstFrost,
        },
        usage(Some(Interval::months(6))),
        Schedule::OneOff {
            due: date(2026, 12, 1),
        },
    ];
    for s in all {
        let j = serde_json::to_string(&s).unwrap();
        let back: Schedule = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s, "{j}");
    }
    let j = serde_json::to_string(&Schedule::Floating {
        every: Interval::months(3),
    })
    .unwrap();
    assert_eq!(j, r#"{"kind":"floating","every":"3 months"}"#);
}
