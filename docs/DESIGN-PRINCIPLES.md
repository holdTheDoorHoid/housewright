# Design principles

> The best documentation is followed by making things easy.

These rules come from the [human factors review](research/human-factors-review.md)
and the [competitor survey](research/competitor-survey.md). Every feature is
judged against them. Where a principle and a feature request conflict, the
principle wins unless the decision log says otherwise.

## The principles, strongest evidence first

**P1. One tap to log.** "Done" works straight from the notification, the QR
sticker, or the task list. Who, when and which item are filled in already.
Notes, photos and costs are offered *after* completion and are never required.
Typing is what makes people quit tracking apps.

**P2. Recommend, explain, let them change it.** Every task ships with a
recommended interval, where that recommendation comes from (manufacturer, NFPA,
template author), and 3 to 7 short steps. Changing the interval is one control.
Where a Home Assistant sensor exists, real usage beats the calendar.

**P3. Start small, grow the plan.** After onboarding only the top 3 to 5 tasks
are switched on, life safety first. More tasks arrive one at a time, when the
season or a just-in-time question makes them relevant. Nobody lands on an
80-item list. Progress is shown honestly ("12 of your systems recorded").

**P4. Put the cue where the work is.** QR stickers on the appliance, tasks
anchored to existing routines ("after the bins go out on Sunday"), and seasonal
landmarks (clock change, first frost, new year). Yearly tasks never become
habits; for those the phone *should* do the remembering.

**P5. Reminders are specific, varied and capped.** Name the item, where it is,
why it matters and how long it takes; use the household's own photo; vary the
wording. One reminder at the due time, at most one follow-up, then it moves into
a weekly digest. Quiet hours and a preferred "maintenance window" per person.

**P6. "When exactly?"** For anything longer than a few minutes, rescheduling
asks for a day *and* a time, and offers to put it in the calendar.

**P7. Snooze means "reschedule to a moment".** "This weekend", "when I'm home",
"next hardware-store trip". After about three snoozes, ask: change the interval,
hand it to someone else, mark it not applicable, or find a pro. Dropping a task
must be easy, so nobody fakes a completion to make it go away.

**P8. Every task has one named owner.** A task owned by "the household" is owned
by nobody. The log records who did it. Everyone sees the plan, not only whoever
set it up. Any who-did-what view is private by default and never a leaderboard.

**P9. Every warning comes with a doable action.** One line on why it matters,
with a real source, then the exact steps, the part number and the time it takes.
No scare copy without an action.

**P10. Measure home health, not streaks.** Show coverage ("your home's systems,
up to date this season") and record completeness. No streaks: yearly tasks make
them meaningless, and broken streaks make people quit. Someone returning after
months away sees "welcome back, these 3 matter most", not "37 overdue".

**P11. Make the records pay off.** Warranty alerts, "last serviced" on every
item, the insurance export and the house binder are the visible return on
logging. The inventory builds itself from onboarding photos and completions.

**P12. Learn the reminder policy, openly.** With opt-in anonymous data (D-32),
measure what reminder timing and wording actually works, and publish it.

## Anti-patterns we do not ship

- Required fields to mark a task done.
- Identical repeated notifications, or daily nags for an overdue item.
- Streaks, points or leaderboards between household members.
- Tasks with no owner.
- A red "37 overdue" badge after a lapse.
- Frightening copy with no clear action.
- A questionnaire that doesn't say how long it is.
- An app that itself needs maintenance: sync conflicts the user must resolve,
  re-setup, account fiddling.

## Lessons from the competitor survey

- **Trust is the opening.** Users complain about subscriptions that creep, apps
  that shut down, and data they can't take with them. Local-first, a documented
  open file format with full export and import, and no paywall on the core loop.
- **Templates are the moat.** No open source tool has them; the best-loved
  commercial feature (photo of the label, get the manual) is also the most
  complained-about because matches are wrong. Show how confident a match is and
  let users correct it.
- **Four schedule kinds** (fixed, floating, triggered, one-off) are the emerging
  common vocabulary among Home Assistant integrations; we support them all, plus
  seasonal.
- **No per-seat or per-property pricing.**
- **Test notifications hard.** Reminders are the core promise and are where
  competitors fail most visibly.
