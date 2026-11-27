> **Provenance.** Prepared 2026-11-27 by AI research agents (Claude) using web search, for the Housewright design. Sources marked as checked were looked up at the time; anything marked UNVERIFIED or (mem) has not been. Verify a citation before relying on it.

# Human factors and behavioural evidence for a home maintenance app

A literature review to shape the design of an open source home maintenance app (Android, iOS, web). The app sends recurring reminders (daily to yearly, sometimes triggered by Home Assistant sensors), keeps an inventory with manuals and warranties, logs completed tasks (notes, photos, who did it), and onboards novices with a questionnaire that builds a prioritised checklist.

Owner's principle: *"the best documentation is followed by making things easy."* The goal is that households keep following the plan and keep the records accurate for months and years without abandoning the app.

Prepared 2026-11-27. About 50 web searches were used to check citations.

---

## How to read the evidence grades

Each source carries one grade.

| Grade | Meaning |
|---|---|
| **MA** | Meta-analysis or systematic review of experiments |
| **RCT** | Randomised controlled trial or field experiment, including micro-randomised trials and "megastudies" |
| **FS** | Field or observational study, survey, natural experiment, or qualitative HCI study |
| **LAB** | Laboratory experiment |
| **TH** | Theory, conceptual model, or design framework |
| **OP** | Opinion, industry or vendor report, practitioner guidance |

**Verification.** Every citation marked ✓ had its authors, venue and year checked against a publisher, PubMed, a repository or an author page during this review. Citations marked (mem) are standard references cited from memory. Their details are well known but were not re-checked in this pass. Nothing here is invented. Where a claim could not be verified, it is left out or flagged.

**One overarching caution.** Behavioural science has had a hard decade. Several famous effects have shrunk or vanished under preregistered replication. Treat any single effect size below as an upper bound on what this app will see. See §9.

---

## 1. Prospective memory and reminders

### What the science says

**Remembering to do something later is "prospective memory".** It fails often, and it fails more for *time-based* intentions ("in March") than for *event-based* intentions that are cued by something you notice ("when I see the furnace").

- **Einstein & McDaniel (2005)**, "Prospective memory: Multiple retrieval processes," *Current Directions in Psychological Science* 14(6):286–290. ✓ **TH/review of LAB.** Remembering relies on two processes: effortful monitoring, and spontaneous retrieval triggered by a *focal* cue. A focal cue is one you are already processing as part of what you are doing. Focal cues need little effort. Nonfocal cues and pure time-based intentions depend on monitoring, which people do badly.
  - *Implication:* a reminder should arrive at the moment and place where the task can be done, and it should name the object involved ("the filter in the hallway return vent"), not an abstract chore.

**Reminders work, modestly, and specific reminders work better than generic ones.**

- **Karlan, McConnell, Mullainathan & Zinman (2016)**, "Getting to the top of mind: How reminders increase saving," *Management Science* 62(12):3393–3411, doi:10.1287/mnsc.2015.2296. ✓ **RCT** (three countries). Savings reminders raised saving by about 6% and goal attainment by about 3 percentage points. In Peru, reminders that named the specific goal raised saving about 16%. Generic reminders had no significant effect.
  - *Implication:* "Time to flush the water heater, so sediment doesn't shorten its life" should beat "You have 1 task due."
- **Calzolari & Nardotto (2017)**, "Effective reminders," *Management Science* 63(9):2915–2932. ✓ **RCT.** Weekly email reminders raised gym attendance. Responses came within hours and recurred with each reminder, which fits a *limited attention* model. Effects after the reminders stopped were small. A 2020 *Experimental Economics* replication found about +13% weekly visits.
  - *Implication:* reminders mostly act on attention in the moment. Do not expect them to build lasting habits by themselves.
- **Thakkar et al. (2016)**, "Mobile telephone text messaging for medication adherence in chronic disease: a meta-analysis," *JAMA Internal Medicine* 176(3):340–349, doi:10.1001/jamainternmed.2015.7667. ✓ **MA** of 16 RCTs (n=2,742). Odds of adherence about doubled (OR 2.11). After adjusting for publication bias it was OR 1.68. The trials were short, adherence was mostly self-reported, and heterogeneity was moderate (I²=62%).
- **Vervloet et al. (2012)**, "The effectiveness of interventions using electronic reminders to improve adherence to chronic medication," *JAMIA* 19(5):696–704, doi:10.1136/amiajnl-2011-000748. ✓ **Systematic review** of 13 studies. There is evidence for *short-term* effects, especially from SMS reminders. Long-term effects are unclear.
- **Milkman et al. (2022)**, "A 680,000-person megastudy of nudges to encourage vaccination in pharmacies," *PNAS* 119(6):e2115126119. ✓ **RCT (megastudy).** Twenty-two text-message variants raised vaccination by an average of 2.0 percentage points (6.8%). The best message said the vaccine was "waiting for you." Neither experts nor lay forecasters predicted the winner.
  - *Implication:* this is the closest analogue for a yearly preventive task. Expect single-digit percentage-point effects, framing a task as already set up for the person helps, and the app should A/B test its own copy rather than trust intuition.

**Cue design: distinctive cues tied to the place where the task is done.**

- **Rogers & Milkman (2016)**, "Reminders through association," *Psychological Science* 27(7):973–986. ✓ **LAB + FS.** Tying an intention to a *distinctive* cue that appears when the person can act raised follow-through substantially. In one lab study, 74% of those given a cue followed through versus 42% without. In a retail field test, coupon use was 24% versus 17%. People undervalue and underuse this technique.
  - *Implication:* offer printable QR or NFC stickers to put on the appliance itself ("scan me when you change this filter"). A scan both cues the task and logs it in one action.

### Habituation and reminder fatigue

- **Anderson, Vance, Kirwan, Jenkins & Eargle (2016)**, "From warning to wallpaper: Why the brain habituates to security warnings and what can be done about it," *Journal of Management Information Systems* 33(3):713–743. ✓ **LAB (fMRI + mouse tracking).** Repeated identical warnings rapidly lose neural attention. *Polymorphic* warnings, whose appearance varies, resisted habituation.
- **Vance et al. (2018)**, *MIS Quarterly* 42(2). ✓ (venue checked; title from mem: "Tuning out security warnings: A longitudinal examination of habituation through fMRI, eye tracking, and field experiments"). **LAB + FS.** In a three-week field experiment, adherence fell more slowly with polymorphic warnings.
  - *Implication:* do not send the same notification text every time. Vary the wording, include the specific item, and add a photo of the household's own appliance.
- **Ancker et al. (2017)**, "Effects of workload, work complexity, and repeated alerts on alert fatigue in a clinical decision support system," *BMC Medical Informatics and Decision Making* 17:36, doi:10.1186/s12911-017-0430-8. ✓ **FS** (112 clinicians, 3.5 years). Clinicians accepted fewer alerts when the alerts *repeated for the same patient* and when cases were complex. Total workload did not drive acceptance. Newly deployed reminders did not show the decline over time that classic desensitisation predicts.
  - *Implication:* the main culprit is the *same item nagging again and again*. Cap repeat reminders per task, and escalate the channel instead of repeating.
- **Mehrotra, Pejovic, Vermeulen, Hendley & Musolesi (2016)**, "My phone and me: Understanding people's receptivity to mobile notifications," *CHI '16*:1021–1032, doi:10.1145/2858036.2858566. ✓ **FS** (in-situ experience sampling). Receptivity depends on the notification's content, its sender and the user's current context, not only its timing. I verified only the abstract-level findings.
- **Klasnja et al. (2019)**, "Efficacy of contextually tailored suggestions for physical activity: A micro-randomized optimization trial of HeartSteps," *Annals of Behavioral Medicine* 53(6):573–582. ✓ **RCT (micro-randomised).** A suggestion raised step count in the next 30 minutes by about 14% (p=.06, marginal). The trial was designed to test whether the effect decays with exposure. I could not confirm a decay result from the abstract, so treat "push effects fade" as plausible and well motivated, not proven by this trial.

### Snooze and deferral

- **Weber, Voit, Auda, Schneegass & Henze (2018)**, "Snooze! Investigating the user-defined deferral of mobile notifications," *MobileHCI '18*, doi:10.1145/3229434.3229436. ✓ (venue and DOI checked; author list from mem). **FS** (year-long in-the-wild deployment, 295 active users, plus interviews). People do use deferral, and they want to defer to a *meaningful moment* ("when I'm home", "Saturday"), not by a fixed interval. The study measured deferral behaviour, not whether tasks got done.
- **Evidence gap.** I found no study that links snooze use in reminder apps to whether the task was eventually completed. A large analysis of alarm-clock snoozing (*Scientific Reports*, 2025; details not fully verified) shows snoozing is the norm for alarms, not the exception.
  - *Implication:* design snooze as *rescheduling to a context or day* ("this weekend", "next time I'm at the hardware store"). Count repeated snoozes as a signal that the task is badly timed or not valued. After the third snooze, ask the user whether to change the interval, hand the task to someone else, or drop it.

### Optimal frequency

**No good study gives an "optimal" reminder frequency for chores.** The evidence above suggests:

- one well-timed, specific reminder per due item;
- at most one or two follow-ups, escalating in channel rather than repeating;
- a weekly digest for everything else.

This is inference, not a measured optimum. The app should collect its own data (§10).

---

## 2. Habit formation, implementation intentions and anchoring

- **Lally, van Jaarsveld, Potts & Wardle (2010)**, "How are habits formed: Modelling habit formation in the real world," *European Journal of Social Psychology* 40(6):998–1009, doi:10.1002/ejsp.674. ✓ **FS** (n=96; 82 analysed).
  - Automaticity grew along an asymptotic curve. Reaching 95% of the plateau took 18–254 days; the median of about 66 days is widely reported (mem).
  - Missing a single opportunity did not materially harm habit formation.
  - The behaviours were *daily*, performed in a *consistent context*, and about half of participants had not reached a high automaticity score by 84 days.
  - *Implication for this app:* true habit formation is realistic only for daily or weekly tasks (wiping the range hood, emptying the dehumidifier). **Quarterly and yearly tasks will never become habits.** They need reminders, anchors and records instead. The finding that one miss does not matter is also a strong argument against punishing a broken streak.
- **Gollwitzer & Sheeran (2006)**, "Implementation intentions and goal achievement: A meta-analysis of effects and processes," *Advances in Experimental Social Psychology* 38:69–119, doi:10.1016/S0065-2601(06)38002-1. ✓ **MA** (94 tests, more than 8,000 participants). Implementation intentions ("If situation X, then I will do Y") had a medium-to-large effect on goal attainment (d≈0.65). Like most meta-analyses from that period it predates modern bias correction (see §9), so the true effect is probably smaller. The direction is very well replicated.
- **Milkman, Beshears, Choi, Laibson & Madrian (2011)**, "Using implementation intentions prompts to enhance influenza vaccination rates," *PNAS* 108(26):10415–10420. ✓ **RCT.**
  - Writing down a *date and time* raised vaccination by 4.2 percentage points (from 33.1%), which was significant.
  - Writing down *only a date* raised it by 1.5 points, which was not significant.
  - *Implication:* when a yearly task comes due, ask "When will you do it?" and require a day plus a time slot, ideally one that goes into the calendar. A date alone is weaker.
- **Judah, Gardner & Aunger (2013)**, "Forming a flossing habit: An exploratory study of the psychological determinants of habit formation," *British Journal of Health Psychology* 18(2):338–353. ✓ **RCT (small, n=50, exploratory).** Flossing *after* brushing produced stronger habits at 8 months than flossing *before*. The authors suggest the boundary at the end of an existing routine is a better anchor than its start.
  - This is the only direct evidence for "habit stacking" I could verify, and it is small. The popular "habit stacking" literature is **OP**.
  - *Implication:* anchor tasks to the *end* of existing routines ("after you take the bins out on Sunday night, check the sump pump").
- **Stawarz, Cox & Blandford (2015)**, "Beyond self-tracking and reminders: Designing smartphone apps that support habit formation," *CHI '15*:2653–2662, doi:10.1145/2702123.2702230. ✓ **RCT (4-week) + review of 115 apps.** This is the most directly relevant study for this app:
  - **reminders increased repetition but *hindered* habit formation**, because people outsourced the remembering to the phone;
  - **event-based cues** (implementation intentions tied to an existing routine) increased automaticity;
  - positive reinforcement (congratulatory messages) had little effect;
  - most apps rely on self-tracking and reminders and lack event-based cues.
  - *Implication:* for frequent tasks, let users *attach the task to a routine* and fade the reminders as automaticity grows. For infrequent tasks, the phone *should* hold the memory; that outsourcing is the point.
- **Temporal landmarks.** Dai, Milkman & Riis (2014), "The fresh start effect: Temporal landmarks motivate aspirational behavior," *Management Science* 60(10):2563–2582, doi:10.1287/mnsc.2014.1901. ✓ **FS** (archival). Goal pursuit rises after landmarks: the new week, month or year, birthdays and holidays.
  - *Implication:* the season changes, the clock changes, the first cold snap, the new year and the anniversary of moving in are natural moments to bundle tasks and to re-engage users who have lapsed ("New season, fresh start: three spring jobs").
- **Seasonal anchors such as "change your clocks, test your smoke alarm".** These are widely promoted. The US campaign has run since 1987 and was sponsored by a battery maker. I found **no independent outcome evaluation**. The only figure found: a UK Fire Kills survey (Ipsos, 2011–12) reported that 14% of respondents tested their alarm over the clock-change weekend, which is self-reported (gov.uk). **Grade: OP/FS, weak.**
  - The anchor is still sound in theory: it is a salient, shared, dated event that is focal (Einstein & McDaniel), and it acts as a fresh-start landmark (Dai et al.).
  - Note that many jurisdictions and newer alarms use sealed 10-year batteries, so the right task can be "check the alarm's expiry date", not "change the battery."

---

## 3. Friction, defaults and logging burden

### Defaults and choice architecture

- **Jachimowicz, Duncan, Weber & Johnson (2019)**, "When and why defaults influence decisions: A meta-analysis of default effects," *Behavioural Public Policy* 3(2):159–186. ✓ **MA** (58 studies, n=73,675).
  - Pooled d≈0.68, but with very large heterogeneity (I²≈98%), and some defaults had no effect or a reverse effect.
  - Defaults work best when users read them as an *endorsement* ("this is what experts recommend") or as the *status quo*.
  - *Implication:* pre-filled schedules ("Recommended: replace every 90 days") are among the strongest tools available. Label them as recommendations and say where they come from: the manufacturer's manual, or a fire service.
- **Thaler & Sunstein (2008)**, *Nudge* (Yale University Press). (mem) **TH/OP.** Background framing only.
- **Johnson & Goldstein (2003)**, "Do defaults save lives?" *Science* 302:1338–1339. (mem) **FS.** This is the classic default demonstration (organ donation).

### Effort, data-entry burden and abandonment

- **Li, Dey & Forlizzi (2010)**, "A stage-based model of personal informatics systems," *CHI '10*:557–566, doi:10.1145/1753326.1753409. ✓ **TH + FS** (surveys and interviews).
  - Five stages: preparation, collection, integration, reflection, action.
  - Barriers in one stage cascade into the later ones.
  - *Implication:* if *collection* (logging) is costly, *reflection* (history, warranty lookup, resale record) never becomes useful, and the payoff that justifies logging disappears.
- **Epstein, Ping, Fogarty & Munson (2015)**, "A lived informatics model of personal informatics," *UbiComp '15*, doi:10.1145/2750858.2804250. ✓ (title, venue and DOI checked; author list from mem). **TH + FS.**
  - Adds *lapsing* and *resuming* as normal phases of tracking.
  - People stop and restart; lapses are expected, not failure.
  - *Implication:* design for return. When a user comes back after months away, offer a "welcome back" triage, not a wall of 37 overdue items.
- **Epstein, Kang, Pina, Fogarty & Munson (2016)**, "Reconsidering the device in the drawer: Lapses as a design opportunity in personal informatics," *UbiComp '16*. ✓ (title and authors checked via University of Washington page). **FS.** Treats lapses as a design opportunity, not just a failure.
- **Krebs & Duncan (2015)**, "Health app use among US mobile phone owners: A national survey," *JMIR mHealth and uHealth* 3(4):e101. ✓ **FS** (n=1,604, online panel, not representative). 45.7% of people who downloaded health apps had stopped using some of them. The main reasons were **high data-entry burden**, loss of interest, and hidden costs.
- **Lazar, Koehler, Tanenbaum & Nguyen (2015)**, "Why we use and abandon smart devices," *UbiComp '15*:635–646, doi:10.1145/2750858.2804288. ✓ **FS** (qualitative, 2 months).
  - Devices were abandoned when they didn't fit the user's self-image, when the data seemed useless, or when **upkeep became unmanageable**.
  - Devices were kept when they had become part of a routine.
  - The authors question whether long-term sustained use is always the right goal.
  - This is relevant because an app *about* maintenance must not itself become a maintenance burden.
- **Choe et al. (2017)**, "Semi-automated tracking: A balanced approach for self-monitoring applications," *IEEE Pervasive Computing* 16(1):74–84, doi:10.1109/MPRV.2017.18. ✓ **TH.**
  - Combine automatic capture with light manual confirmation.
  - Fully manual tracking is burdensome. Fully automatic tracking loses awareness and trust.
  - *Implication:* Home Assistant sensors are the "automatic" half (furnace run-hours, water-softener salt level, leak sensors). One tap to confirm "I did it" is the manual half.
- **Fogg (2009)**, "A behavior model for persuasive design," *Persuasive '09*. (mem) **TH.** Behaviour happens when motivation, ability and a prompt converge. Raising ability, by making the task easier, is usually cheaper than raising motivation. This is popular in industry but has little direct experimental testing as a model. It fits the owner's principle exactly.

**Synthesis for logging.** The evidence on abandonment consistently points to *capture burden*. The default log entry must be **one tap from the notification itself** ("Done", "Done, add note", "Snooze to…"). Who did it, when, and which item should be pre-filled. Photos and notes are optional extras, offered *after* completion, never required to complete.

---

## 4. Gamification, streaks and rewards

- **Koivisto & Hamari (2019)**, "The rise of motivational information systems: A review of gamification research," *International Journal of Information Management* 45:191–210. ✓ **Systematic review** (819 studies). Results lean positive but are remarkably mixed. Research models and constructs are inconsistent. An earlier review is Hamari, Koivisto & Sarsa (2014), *HICSS* ✓.
- **Deci, Koestner & Ryan (1999)**, "A meta-analytic review of experiments examining the effects of extrinsic rewards on intrinsic motivation," *Psychological Bulletin* 125(6):627–668. ✓ **MA** (128 studies).
  - Tangible rewards that are expected and contingent on doing a task reduced free-choice intrinsic motivation (d≈−0.28 to −0.40).
  - This is **contested**: Eisenberger & Cameron and Cameron & Pierce argued the effects are narrower.
  - Unexpected rewards and verbal praise did not undermine motivation in the same way.
  - *Implication:* points and badges are a poor fit for an intrinsically sensible goal like "protect my home." If used, they should be *informational* ("Your roof was last inspected 14 months ago; your records are complete"), not controlling.
- **Ryan & Deci (2000)**, "Self-determination theory and the facilitation of intrinsic motivation…," *American Psychologist* 55(1):68–78. (mem) **TH.** People's needs for autonomy, competence and relatedness drive lasting motivation.
- **Hanus & Fox (2015)**, "Assessing the effects of gamification in the classroom: A longitudinal study on intrinsic motivation, social comparison, satisfaction, effort, and academic performance," *Computers & Education* 80:152–161, doi:10.1016/j.compedu.2014.08.019. ✓ **Quasi-experiment** (16 weeks). Badges and a leaderboard *reduced* motivation, satisfaction and empowerment over time, and the gamified group scored lower on the final exam. It is a single course, but a cautionary example of leaderboards backfiring over long horizons.
- **Silverman & Barasch (2023)**, "On or off track: How (broken) streaks affect consumer decisions," *Journal of Consumer Research* 49(6):1095–1117. ✓ **LAB + FS** (7 studies).
  - Showing an intact streak raises continued engagement. Showing a *broken* streak lowers it, *regardless of actual past behaviour*.
  - The drop is larger when people blame themselves for the break.
  - It is smaller when the streak can be "repaired."
  - The authors advise apps not to dwell on broken streaks.
- **Milkman et al. (2021)**, "Megastudies improve the impact of applied behavioural science," *Nature* 600:478–483. ✓ **RCT (megastudy**, n≈61,000 gym members, 53 interventions).
  - 45% of interventions raised visits during the 4-week programme, by 9–27%.
  - The best performer was a micro-reward for *returning after a missed workout*.
  - **Only about 8% had significant effects after the programme ended.**
  - Expert forecasts did not predict which interventions worked.
- **Nunes & Drèze (2006)**, "The endowed progress effect: How artificial advancement increases effort," *Journal of Consumer Research* 32(4):504–512. ✓ **FS/LAB.** A loyalty card with 2 of 10 stamps pre-filled was completed more often and faster than an 8-stamp card, even though both required eight purchases.
  - *Implication:* after onboarding, show progress already made ("You've recorded 12 of your home's systems; 4 to go"). Use honest progress only, from things the user actually did.

### Why streaks fit badly with quarterly and yearly tasks

A streak needs a cadence fast enough that the user feels it, and Lally et al. showed that missing one occurrence does no harm to habit formation. A yearly gutter clean gives a "streak" of one event per year. Breaking it is both likely and, per Silverman & Barasch, demotivating.

Better alternatives:

- **Coverage or "health" metrics:** the percentage of the home's systems that are up to date this season.
- **Record completeness:** "Every appliance has a manual and purchase date."
- **Seasonal bundles** completed as a set, with a fresh start each season.
- **Return rewards:** recognising a *comeback* is the one gamification mechanic that tested best in the gym megastudy.

---

## 5. Shared responsibility in households

- **Darley & Latané (1968)**, "Bystander intervention in emergencies: Diffusion of responsibility," *Journal of Personality and Social Psychology* 8(4):377–383. (mem) **LAB.** The more people who could act, the less likely any one person is to act.
- **Karau & Williams (1993)**, "Social loafing: A meta-analytic review and theoretical integration," *Journal of Personality and Social Psychology* 65(4):681–706. ✓ **MA** (78 studies). Social loafing is robust and moderate in size. It is reduced when individual contributions are identifiable and evaluated, and when the task is meaningful. These moderators come from the Collective Effort Model; check the original paper for the exact moderator estimates.
  - *Implication:* a task assigned to "the household" belongs to nobody. Every task should have **one named owner** (rotating if wanted). The completion log should record *who* did it, which also supports the reliability of the records.
- **Daminger (2019)**, "The cognitive dimension of household labor," *American Sociological Review* 84(4):609–633. ✓ **FS** (qualitative, 35 couples).
  - Household cognitive labour has four stages: *anticipate, identify options, decide, monitor*.
  - Women did more of the anticipating and monitoring. Deciding was more evenly shared.
  - *Implication:* the app takes over *anticipating* and *monitoring*, the invisible "mental load." That is a large and under-appreciated benefit. The flip side is that whoever *configures* the app can become the default manager. Show all household members the plan, not just the person who set it up.
- **Perceived fairness.** Perceived inequity in housework is associated with lower relationship satisfaction (for example a PLOS ONE 2019 study, n≈10,000, of gendered perceptions of fairness in housework; a Penn State study on dual-earner couples linking perceived inequity to marital happiness and to divorce among wives). ✓ (existence checked, details from abstracts) **FS (correlational).**
  - *Implication:* a "who did what" view helps transparency. It can also become a scoreboard that *fuels* conflict. Show balance gently and privately, and let households turn it off.
- **Chore-app research.** I found no strong experimental HCI evidence on chore apps for couples. Journalism (MIT Technology Review, 2022; MIT Sloan Management Review) notes that most chore apps assume one person delegates to others, which can reinforce imbalance. **Grade: OP.**
- **Harkin et al. (2016)**, "Does monitoring goal progress promote goal attainment? A meta-analysis of the experimental evidence," *Psychological Bulletin* 142(2):198–229. ✓ **MA** (138 studies, n≈19,951).
  - Monitoring progress improved goal attainment (d≈0.40).
  - Effects were larger when progress was **physically recorded** and **reported to others or made public**.
  - *Implication:* this is the strongest evidence that a shared, visible maintenance log does real work, beyond being a record.
- **Lerner & Tetlock (1999)**, "Accounting for the effects of accountability," *Psychological Bulletin* 125(2):255–275. (mem) **Review.** Accountability improves effort mainly when the audience's views are unknown and the accountability is expected in advance. It can also produce defensive box-ticking.

---

## 6. Risk perception, prioritisation and present bias

### Why people under-maintain

- **O'Donoghue & Rabin (1999)**, "Doing it now or later," *American Economic Review* 89(1):103–124. ✓ **TH.** People with present bias put off tasks whose costs come now and whose benefits come later. People who are unaware of their own present bias ("naive" agents) can procrastinate indefinitely. Preventive maintenance is exactly that kind of task. **Laibson (1997)**, *Quarterly Journal of Economics* (mem), gives the quasi-hyperbolic model behind it.
- **Meyer & Kunreuther (2017)**, *The Ostrich Paradox: Why We Underprepare for Disasters* (Wharton Digital Press). ✓ **TH/review** (book).
  - Six biases drive under-preparation. I verified myopia, amnesia and optimism; the full list is in the book.
  - Their prescription: design *around* the biases with framing, defaults and incentives, rather than hoping to educate them away.
- **Homeowner surveys.** **OP/FS, mostly sponsored by insurers or vendors.**
  - A Nationwide survey reported that 44% of homeowners delayed routine maintenance in the past year, citing cost (78% of those), other financial priorities, or lack of time or energy.
  - A Hartford study found the top reason for skipping a task was that "it isn't much of a problem right now."
  - US Census Bureau Statistical Brief SB/94-07 (1994), using American Housing Survey data, is an old government baseline on maintenance spending.
  - **Peer-reviewed research on household preventive maintenance behaviour is thin.** That is a genuine evidence gap.

### What is at stake (useful for "why this matters" copy)

**Fire.** NFPA, *Smoke Alarms in US Home Fires*. ✓ **FS** (national fire statistics).

- In 2012–2016, about three in five home fire deaths occurred in homes with no smoke alarm (40%) or no *working* one (17%).
- Where alarms were present but did not operate (2007–2011 data), 47% had missing or disconnected batteries and 24% had dead batteries.
- The risk of dying is roughly halved with working alarms.
- The newest NFPA reports update these numbers. Cite the current edition in the app.

**Water.** IBHS (Insurance Institute for Business & Home Safety) studies of plumbing and appliance failures are cited secondhand.

- Washing-machine hose and supply-line failures commonly cost more than $5,000 per incident after the deductible.
- I could **not** retrieve the primary IBHS report, so the exact figures are unverified. **Grade: OP (secondary).**

**Sensors.** The LexisNexis Risk Solutions and Flo by Moen study (about 2020, 2,306 homes) reported up to a 96% drop in paid water claims for homes with an automatic shut-off valve.

- It was **vendor-co-funded** and prone to selection bias, because the homes that buy sensors differ from those that don't.
- A Nationwide and Resideo analysis reported lower frequency and severity of claims.
- **Grade: FS/OP.** Directional only.

**Records.** The Insurance Information Institute reported (2015–2016 surveys) that only about half of homeowners have a home inventory. ✓ (press releases) **FS/OP.** This supports the value of the inventory feature for insurance claims.

### How to frame risk

- **Tannenbaum et al. (2015)**, "Appealing to fear: A meta-analysis of fear appeal effectiveness and theories," *Psychological Bulletin* 141(6):1178–1204. ✓ **MA.** Fear appeals have a small but consistent positive effect. No evidence was found that they backfire relative to a control. They work better when paired with **efficacy**: a clear, doable recommended action.
- **Peters, Ruiter & Kok (2013)**, "Threatening communication: A critical re-analysis and a revised meta-analytic test of fear appeal theory," *Health Psychology Review* 7(suppl 1):S8–S31, doi:10.1080/17437199.2012.703527. ✓ **MA** (only 6 studies met strict criteria). Threat helped *only when efficacy was high*, and efficacy helped only when threat was high.
  - *Implication:* every "why this matters" message must be paired with "here's exactly how, in about 10 minutes, with this part number." A scary message with no clear action is the anti-pattern.
- **Gallagher & Updegraff (2012)**, "Health message framing effects on attitudes, intentions, and behavior: A meta-analytic review," *Annals of Behavioral Medicine* 43(1):101–116, doi:10.1007/s12160-011-9308-7. ✓ **MA.**
  - Gain-framed messages had a small advantage for *prevention behaviour* (r≈.08, from the corrected erratum).
  - Loss framing had no reliable advantage for detection behaviours.
  - Framing effects are small overall.
  - *Implication:* "loss framing is more persuasive" is **not** well supported for prevention. Use concrete, honest stakes (cost of neglect, safety) with a gain angle ("keeps your furnace running efficiently, adds years of life"). Do not invest in elaborate framing A/B tests expecting large wins.
- **Kahneman & Tversky (1979)**, "Prospect theory," *Econometrica* 47(2):263–291. (mem) **TH.** Loss aversion is real in choice experiments. Turning it into persuasive messaging is a separate claim, and Gallagher & Updegraff show that translation is weak.

### Prioritisation

The app must rank tasks for novices. Risk science suggests sorting by:

1. **Life safety:** smoke and CO alarms, dryer vents, gas appliances.
2. **High-severity, high-frequency property loss:** water supply lines, water heaters, roof, gutters and drainage.
3. **Efficiency and lifespan:** HVAC filters, coils.
4. **Cosmetic tasks.**

Show the *reason* for each task's rank. This also turns the default into an *endorsement*, which per Jachimowicz et al. is the kind of default that works best.

---

## 7. Onboarding and overwhelm

- **Choice overload is contested.**
  - Iyengar & Lepper (2000), "When choice is demotivating," *JPSP* 79(6):995–1006 (mem) **LAB/FS**, is the classic jam study.
  - **Scheibehenne, Greifeneder & Todd (2010)**, *Journal of Consumer Research* 37(3):409–425, doi:10.1086/651235. ✓ **MA** (50 experiments). The mean effect is **about zero**, with large variance between studies.
  - **Chernev, Böckenholt & Goodman (2015)**, *Journal of Consumer Psychology* 25(2):333–358, doi:10.1016/j.jcps.2014.08.002. ✓ **MA** (99 observations). Overload appears reliably under four moderators: **task difficulty, choice-set complexity, preference uncertainty, and an effort-minimising goal**.
  - A novice homeowner facing 80 maintenance tasks meets *all four*. So the risk of overwhelm is real for this specific user, even though "more options always hurt" is not a general law.
- **Questionnaire length.** **Galesic & Bosnjak (2009)**, "Effects of questionnaire length on participation and indicators of response quality in a web survey," *Public Opinion Quarterly* 73(2):349–360, doi:10.1093/poq/nfp031. ✓ **RCT.**
  - Longer *announced* length reduced starts (75% for 10 minutes versus 65% for 20 minutes).
  - Answers later in long questionnaires were faster, shorter and more uniform: lower quality, more "straight-lining."
  - *Implication:* keep the first-run questionnaire short (a few minutes), state its length up front, and ask the highest-value questions first. Later questions will be answered worse, so defer them to "just-in-time" prompts.
- **Endowed progress (Nunes & Drèze 2006)** and **goal monitoring (Harkin et al. 2016)** support showing a visible, partially filled progress indicator for setup.
- **Progressive disclosure** (Nielsen Norman Group guidance). **OP.** Widely accepted practice, with little controlled evidence.

### Recommended onboarding shape (synthesis)

1. Ask about **8–12 high-value questions** in a few minutes: home type, heating type, water heater type, the presence of a sump, gas, a fireplace, smoke and CO alarms, a well or septic system.
2. Build the full plan silently, but **activate only the top 3–5 tasks** (life safety first, then the riskiest water item).
3. Unlock more tasks as early ones are completed. For example, one new system "discovered" per week, prompted by a just-in-time question such as "Do you have a dehumidifier?" when the season makes it relevant.
4. Let users reach the full list if they want it (autonomy), but never land them on it.

---

## 8. Lessons from adjacent fields

### Medication adherence

Medication adherence is the most studied reminder domain.

- Reminders improve short-term adherence (Thakkar 2016 MA; Vervloet 2012 SR).
- Long-term effects are unclear.
- Self-reported outcomes inflate the effects.
- Simple, specific, timely messages beat complex ones.
- *Transfer:* reminders are necessary but not sufficient. Pair them with a cue in the environment, such as a sticker on the appliance, and with low-friction confirmation.

### Mobile app retention generally

- **Baumel, Muench, Edan & Kane (2019)**, "Objective user engagement with mental health apps," *Journal of Medical Internet Research* 21(9):e14567. ✓ **FS** (panel usage data, 93 apps). The median daily open rate was about 4%. The median 30-day retention for most app categories was in single digits.
- *Transfer:* most users abandon most apps within weeks. A maintenance app that only needs attention a few times a month must **deliver value without being opened**: the notification is the interface, the calendar holds the plan, and the log builds itself from sensors.

### Industrial preventive maintenance (CMMS)

- The practitioner literature describes "PM overload" and **pencil-whipping**: signing off tasks that were never done, especially when schedules are unrealistic, tasks seem pointless, or compliance metrics are lagging. **Grade: OP** (vendor and trade sources such as MPulse and Siemens). I found no peer-reviewed study that quantifies it.
- Reliability-centred maintenance practice recommends adjusting PM intervals using failure history rather than fixed calendars. **OP.**
- *Transfer:*
  - Too many low-value tasks get *fake-logged*, which corrupts the record the app exists to keep.
  - Trim the default plan aggressively.
  - Let users say "not applicable" or "less often" easily.
  - Prefer usage-based intervals from sensors (filter by fan run-hours, not by calendar) where available.
  - Optional photo evidence for important tasks discourages pencil-whipping without making it mandatory.

### Checklists in aviation and medicine

- **Degani & Wiener (1993)**, "Cockpit checklists: Concepts, design, and use," *Human Factors* 35(2):345–359. ✓ **FS + TH** (field study, design guidelines).
  - Checklists fail through improper use or non-use.
  - Long checklists invite skimming.
  - Design must fit real workflow and culture.
  - Critical items should be few and placed where they cannot be skipped.
- **Haynes et al. (2009)**, "A surgical safety checklist to reduce morbidity and mortality in a global population," *NEJM* 360(5):491–499. ✓ **FS** (before/after, 8 hospitals). Mortality fell from 1.5% to 0.8% and complications from 11.0% to 7.0%. It was **not randomised**.
- **Urbach et al. (2014)**, "Introduction of surgical safety checklists in Ontario, Canada," *NEJM* 370:1029–1038. ✓ **FS** (natural experiment, province-wide). **No significant reduction** in mortality or complications. Critics attribute this to mandated adoption without real implementation, and to compliance that was over-reported (checklists were "pencil-whipped").
- **Gawande (2009)**, *The Checklist Manifesto* (Metropolitan Books). (mem) **OP.** Influential argument for short "killer item" checklists with clear pause points.
- *Transfer:*
  - A checklist works when it is **short, used at the point of action, and owned by the people doing the work**.
  - Imposing a checklist from outside, with compliance that is simply reported, produces box-ticking.
  - Each maintenance task's in-app procedure should be 3–7 steps, in "do" form, shown when the task is opened, with the critical step highlighted (for example "Turn off power at the breaker first").

---

## 9. Where the evidence is weak or contested

1. **Nudge effect sizes shrink at scale.** **DellaVigna & Linos (2022)**, "RCTs to scale: Comprehensive evidence from two nudge units," *Econometrica* 90(1):81–116. ✓
   - Academic nudge papers averaged an 8.7 percentage-point effect (+33%).
   - 126 nudge-unit trials with 23 million people averaged 1.4 points (+8%).
   - About 70% of the gap is explained by selective publication and low power.
   - *Plan for effects in the low single digits per intervention.*
2. **Nudge meta-analyses and publication bias.** Mertens et al. (2022, *PNAS*) reported d≈0.43 for choice-architecture interventions. **Maier et al. (2022)**, "No evidence for nudging after adjusting for publication bias," *PNAS* 119:e2200300119 ✓, found no evidence for an *average* effect after bias correction, while noting that some categories (such as defaults) may still work. Mertens et al. replied with bias-adjusted estimates of d≈0.08–0.31. ✓
   - The fair reading: **average nudges are weak. Defaults and effort reduction are the most robust. Informational and "framing" nudges are the least.**
3. **Ego depletion.**
   - Hagger et al. (2016), *Perspectives on Psychological Science* 11(4):546–573 ✓, a 23-lab registered replication, found an effect near zero.
   - Vohs et al. (2021), *Psychological Science* 32(10):1566–1581 ✓, with 36 labs and n=3,531, also found no significant effect.
   - **Do not design around "willpower runs out"** (for example "schedule tasks in the morning when willpower is high"). Design around *time, attention and effort*, which are well supported.
4. **Deadlines and precommitment.**
   - **Ariely & Wertenbroch (2002)**, *Psychological Science* 13(3):219–224, the classic "self-imposed deadlines" paper, **failed to replicate** (Hyndman & Bisin, *Psychological Science*, July 2026).
   - It was **retracted in September 2026** after data anomalies were found (Retraction Watch, 3 Sept 2026; Data Colada #138). ✓
   - Do not cite it as support for user-set deadlines. The implementation-intentions evidence (§2) is the better basis for "pick a day and time."
5. **Choice overload** is real only under specific moderators (§7). The average effect is about zero.
6. **Overjustification** is contested in scope (Deci et al. 1999 versus Cameron & Pierce). It is most robust for *expected, tangible, task-contingent* rewards.
7. **Habit-stacking** rests on one small study (Judah et al. 2013) plus theory. The popular claims go far beyond the evidence.
8. **Gamification** reviews are mixed and the outcomes are short-term. Long-horizon evidence is scarce and partly negative (Hanus & Fox 2015).
9. **Homeowner maintenance behaviour** is studied mostly through sponsored surveys. Insurer and sensor-vendor figures (IBHS secondary citations, LexisNexis/Flo) carry selection-bias and conflict-of-interest caveats.
10. **Reminder frequency and snooze** have no direct evidence for chore-type tasks with long intervals. This is the biggest gap relevant to the app, and the app can fill it (§10).
11. **The checklist effect** depends on implementation (Haynes versus Urbach). Context and ownership matter more than the artefact.

---

## 10. Design principles, ranked

The ranking reflects strength of evidence × expected impact on long-term adherence and record accuracy.

### P1. One-tap completion from the reminder; everything else optional
**Evidence:** Krebs & Duncan 2015 (data-entry burden is the top reason for quitting); Li et al. 2010 (collection barriers cascade); Lazar et al. 2015 (upkeep burden causes abandonment); Choe et al. 2017 (semi-automation); defaults and effort reduction are the most robust nudges after bias correction (Jachimowicz 2019; Maier 2022; DellaVigna & Linos 2022).

- The notification carries **Done / Done + photo / Reschedule…** actions.
- Who, when and which item are pre-filled. Notes and photos are offered *after* completion.
- Sensor-confirmed tasks log themselves and ask only for confirmation.

### P2. Smart defaults that are endorsed, explained and editable
**Evidence:** Jachimowicz et al. 2019 MA (defaults are strongest when read as endorsements); Meyer & Kunreuther 2017 (design around bias, don't lecture).

- Every task ships with a recommended interval, a source ("manufacturer", "NFPA", "your installer"), and a 3–7 step procedure.
- The user changes the interval with one control. Usage-based intervals from Home Assistant take priority over calendar intervals where a sensor exists.

### P3. Start tiny and grow the plan; never show a novice 80 tasks
**Evidence:** Chernev et al. 2015 (overload under difficulty, complexity, uncertainty and an effort-minimising goal); Galesic & Bosnjak 2009 (length cuts participation and quality); Nunes & Drèze 2006 (endowed progress); CMMS PM-overload practice (OP).

- Use a short questionnaire with its length announced.
- Activate the **top 3–5 tasks**, ranked by life safety, then severe water and fire risks.
- Discover new tasks just in time and seasonally.
- Show honest progress ("4 of your home's systems set up").

### P4. Cue the task at the point of action, not just on the phone
**Evidence:** Einstein & McDaniel 2005 (focal, event-based cues); Rogers & Milkman 2016 (distinctive cues at the place of action); Stawarz et al. 2015 (event cues build automaticity; reminders alone hinder it); Judah et al. 2013 (anchor to the end of a routine).

- Offer printable QR or NFC labels for appliances that open the task and log it.
- Offer "anchor to a routine" for frequent tasks ("after Sunday bins…").
- Offer seasonal and clock-change anchors for infrequent ones.

### P5. Specific, varied, capped reminders that escalate instead of repeating
**Evidence:** Karlan et al. 2016 (specific reminders work, generic ones don't); Anderson et al. 2016 and Vance et al. 2018 (habituation; varied warnings resist it); Ancker et al. 2017 (repeated alerts for the same item drive overrides); Calzolari & Nardotto 2017 (reminders act on attention in the moment).

- Name the item, its location, the reason and the time it takes.
- Vary the wording and add the household's own photo.
- Send one reminder at the due time, at most one follow-up, then move the item into a weekly digest rather than nagging.
- Users choose quiet hours and a preferred "maintenance window."

### P6. At due time, ask "when exactly?" and put the answer in the calendar
**Evidence:** Gollwitzer & Sheeran 2006 MA; Milkman et al. 2011 RCT (date *and* time, not date alone); Milkman et al. 2022 megastudy (framing as "waiting for you").

- For tasks longer than a few minutes, the reschedule action asks for a day and a time slot and offers a calendar export.
- The copy frames the task as already prepared: "Your filter size is saved and the steps are ready."

### P7. Snooze means "reschedule to a meaningful moment"; repeated snoozing triggers a review
**Evidence:** Weber et al. 2018 (people want to defer to a context, not an interval); O'Donoghue & Rabin 1999 (naive procrastination is unbounded); CMMS pencil-whipping (OP).

- Offer "this weekend", "when I'm home" (geofence or Home Assistant presence), and "next hardware-store trip" (attach to a shopping list).
- After about three deferrals, ask: *change the interval, reassign, mark not applicable, or get help (find a pro)?*
- Dropping a task is legitimate and must be easy, so it doesn't turn into fake logging.

### P8. Every task has one named owner; the log records who did it
**Evidence:** Karau & Williams 1993 MA (loafing drops when contributions are identifiable); Darley & Latané 1968 (diffusion); Harkin et al. 2016 MA (recorded, shared progress improves attainment); Daminger 2019 (the invisible anticipating and monitoring labour).

- Assign or rotate an owner for each task; avoid tasks with no owner.
- The app visibly takes over the *anticipating* and *monitoring* work.
- Everyone in the household sees the plan.
- A fairness view is optional, private by default, and never shown as a competitive leaderboard.

### P9. Explain why the task matters with a clear, doable action and honest stakes
**Evidence:** Peters et al. 2013 MA (threat works only with high efficacy); Tannenbaum et al. 2015 MA (small positive fear-appeal effect, better with efficacy); Gallagher & Updegraff 2012 MA (small gain-frame edge for prevention); NFPA smoke-alarm statistics.

- Give a one-line reason backed by a real statistic or source, followed by exact steps, the part number and the time required.
- No scare copy without an action.
- Cite sources so the defaults read as endorsements (P2).

### P10. Measure health, not streaks; reward comebacks; make lapses painless
**Evidence:** Silverman & Barasch 2023 (broken streaks demotivate; repairable streaks less so); Lally et al. 2010 (one miss doesn't matter); Milkman et al. 2021 (micro-reward for returning after a miss performed best); Epstein et al. 2015 and 2016 (lapsing is normal); Dai et al. 2014 (fresh starts).

- Use "Home health" coverage by season and "record completeness" rather than streaks.
- After a lapse, the return screen triages to the top 3 overdue items: "Welcome back; these 3 matter most."
- Hide or collapse the long overdue tail.
- Use seasonal fresh starts.

### P11. Make the records pay off visibly, so logging feels worth it
**Evidence:** Li et al. 2010 (the reflection stage must pay off); Lazar et al. 2015 (devices abandoned when the data isn't useful); Insurance Information Institute (only about half of homeowners have an inventory).

- Warranty expiry alerts, "last serviced" shown on each item, and a one-click insurance or home-sale report.
- When a manual is scanned, its schedule pre-fills the tasks.
- The inventory is a by-product of onboarding and completion photos, not a separate chore.

### P12. Let the app learn its own reminder policy, transparently
**Evidence:** DellaVigna & Linos 2022 and Milkman et al. 2021/2022 (forecasts fail; effects are small; test locally); Klasnja et al. 2019 (micro-randomised trials as a method).

- Instrument completion against reminder timing, channel and wording.
- With opt-in, privacy-preserving telemetry (in line with an open source ethos), run simple A/B tests of reminder copy and timing.
- Publish the results. This fills the biggest gap in the evidence (§9 item 10).

---

## 11. Anti-patterns to avoid

| Anti-pattern | Why (evidence) |
|---|---|
| Landing a novice on a full 60–80 item plan | Overload under complexity and uncertainty (Chernev 2015); CMMS PM overload leads to pencil-whipping (OP) |
| Required fields (photo, notes, cost) to mark a task done | Data-entry burden is the top reason for quitting (Krebs & Duncan 2015); burden cascades (Li 2010); invites fake logging |
| Identical repeat notifications, or daily nags for overdue items | Habituation (Anderson 2016; Vance 2018); repeat-alert fatigue (Ancker 2017) |
| Streaks for monthly, quarterly or yearly tasks; "you broke your streak!" | Broken streaks reduce engagement (Silverman & Barasch 2023); habits form only for frequent tasks in a stable context (Lally 2010) |
| Points, badges and leaderboards between household members | Long-horizon leaderboards reduced motivation (Hanus & Fox 2015); risk of undermining intrinsic motivation (Deci 1999); fairness conflict (§5) |
| Tasks assigned to "everyone" or "household" | Diffusion and loafing (Darley & Latané 1968; Karau & Williams 1993) |
| Scary copy with no clear, doable action | Threat without efficacy doesn't work (Peters 2013) |
| A red "37 overdue" badge after a lapse | Punishes return; lapsing is normal (Epstein 2015/2016); broken-streak effect |
| A fixed-interval snooze button with no reschedule-to-context option | People defer to meaningful moments (Weber 2018); hides badly timed tasks |
| A long upfront questionnaire with no stated length | Fewer starts and worse answer quality (Galesic & Bosnjak 2009) |
| Designing around "willpower" or "self-imposed deadlines" | Ego depletion did not replicate (Hagger 2016; Vohs 2021); Ariely & Wertenbroch 2002 retracted in 2026 |
| Expecting large nudge effects, or trusting the team's intuition about what works | Effects shrink at scale (DellaVigna & Linos 2022); forecasters fail (Milkman 2021) |
| An app that is itself high-maintenance (sync conflicts, frequent re-setup, required account fiddling) | Abandonment through upkeep burden (Lazar 2015) |

---

## 12. Reference list

✓ = checked during this review (authors, venue and year, plus DOI where given). (mem) = standard reference cited from memory, not re-checked in this pass.

- Ancker, J. S., et al. (2017). Effects of workload, work complexity, and repeated alerts on alert fatigue in a clinical decision support system. *BMC Med Inform Decis Mak*, 17:36. doi:10.1186/s12911-017-0430-8 ✓
- Anderson, B. B., Vance, A., Kirwan, C. B., Jenkins, J. L., & Eargle, D. (2016). From warning to wallpaper: Why the brain habituates to security warnings and what can be done about it. *J Management Information Systems*, 33(3), 713–743. ✓
- Baumel, A., Muench, F., Edan, S., & Kane, J. M. (2019). Objective user engagement with mental health apps. *J Med Internet Res*, 21(9):e14567. ✓
- Calzolari, G., & Nardotto, M. (2017). Effective reminders. *Management Science*, 63(9), 2915–2932. ✓
- Chernev, A., Böckenholt, U., & Goodman, J. (2015). Choice overload: A conceptual review and meta-analysis. *J Consumer Psychology*, 25(2), 333–358. doi:10.1016/j.jcps.2014.08.002 ✓
- Choe, E. K., Abdullah, S., Rabbi, M., et al. (2017). Semi-automated tracking: A balanced approach for self-monitoring applications. *IEEE Pervasive Computing*, 16(1), 74–84. doi:10.1109/MPRV.2017.18 ✓
- Dai, H., Milkman, K. L., & Riis, J. (2014). The fresh start effect. *Management Science*, 60(10), 2563–2582. doi:10.1287/mnsc.2014.1901 ✓
- Daminger, A. (2019). The cognitive dimension of household labor. *American Sociological Review*, 84(4), 609–633. ✓
- Darley, J. M., & Latané, B. (1968). Bystander intervention in emergencies: Diffusion of responsibility. *JPSP*, 8(4), 377–383. (mem)
- Deci, E. L., Koestner, R., & Ryan, R. M. (1999). A meta-analytic review of experiments examining the effects of extrinsic rewards on intrinsic motivation. *Psychological Bulletin*, 125(6), 627–668. ✓
- Degani, A., & Wiener, E. L. (1993). Cockpit checklists: Concepts, design, and use. *Human Factors*, 35(2), 345–359. ✓
- DellaVigna, S., & Linos, E. (2022). RCTs to scale: Comprehensive evidence from two nudge units. *Econometrica*, 90(1), 81–116. ✓
- Einstein, G. O., & McDaniel, M. A. (2005). Prospective memory: Multiple retrieval processes. *Current Directions in Psychological Science*, 14(6), 286–290. ✓
- Epstein, D. A., Ping, A., Fogarty, J., & Munson, S. A. (2015). A lived informatics model of personal informatics. *UbiComp '15*. doi:10.1145/2750858.2804250 ✓ (authors partly from mem)
- Epstein, D. A., Kang, J. H., Pina, L. R., Fogarty, J., & Munson, S. A. (2016). Reconsidering the device in the drawer: Lapses as a design opportunity in personal informatics. *UbiComp '16*. ✓
- Fogg, B. J. (2009). A behavior model for persuasive design. *Persuasive '09*. (mem)
- Galesic, M., & Bosnjak, M. (2009). Effects of questionnaire length on participation and indicators of response quality in a web survey. *Public Opinion Quarterly*, 73(2), 349–360. doi:10.1093/poq/nfp031 ✓
- Gallagher, K. M., & Updegraff, J. A. (2012). Health message framing effects on attitudes, intentions, and behavior: A meta-analytic review. *Annals of Behavioral Medicine*, 43(1), 101–116. doi:10.1007/s12160-011-9308-7 ✓
- Gawande, A. (2009). *The Checklist Manifesto*. Metropolitan Books. (mem)
- Gollwitzer, P. M., & Sheeran, P. (2006). Implementation intentions and goal achievement: A meta-analysis of effects and processes. *Advances in Experimental Social Psychology*, 38, 69–119. doi:10.1016/S0065-2601(06)38002-1 ✓
- Hagger, M. S., et al. (2016). A multilab preregistered replication of the ego-depletion effect. *Perspectives on Psychological Science*, 11(4), 546–573. ✓
- Hamari, J., Koivisto, J., & Sarsa, H. (2014). Does gamification work? A literature review of empirical studies on gamification. *HICSS-47*. ✓
- Hanus, M. D., & Fox, J. (2015). Assessing the effects of gamification in the classroom. *Computers & Education*, 80, 152–161. doi:10.1016/j.compedu.2014.08.019 ✓
- Harkin, B., et al. (2016). Does monitoring goal progress promote goal attainment? A meta-analysis of the experimental evidence. *Psychological Bulletin*, 142(2), 198–229. ✓
- Haynes, A. B., et al. (2009). A surgical safety checklist to reduce morbidity and mortality in a global population. *NEJM*, 360(5), 491–499. ✓
- Hyndman, K., & Bisin, A. (2026). Replication of Ariely & Wertenbroch (2002). *Psychological Science*, published online July 2026. doi:10.1177/09567976261460772 ✓ (existence and result; exact title not checked). The original was retracted in September 2026 (Retraction Watch, 3 Sept 2026). ✓
- Iyengar, S. S., & Lepper, M. R. (2000). When choice is demotivating. *JPSP*, 79(6), 995–1006. (mem)
- Jachimowicz, J. M., Duncan, S., Weber, E. U., & Johnson, E. J. (2019). When and why defaults influence decisions: A meta-analysis of default effects. *Behavioural Public Policy*, 3(2), 159–186. ✓
- Johnson, E. J., & Goldstein, D. (2003). Do defaults save lives? *Science*, 302, 1338–1339. (mem)
- Judah, G., Gardner, B., & Aunger, R. (2013). Forming a flossing habit. *British Journal of Health Psychology*, 18(2), 338–353. ✓
- Kahneman, D., & Tversky, A. (1979). Prospect theory. *Econometrica*, 47(2), 263–291. (mem)
- Karau, S. J., & Williams, K. D. (1993). Social loafing: A meta-analytic review and theoretical integration. *JPSP*, 65(4), 681–706. ✓
- Karlan, D., McConnell, M., Mullainathan, S., & Zinman, J. (2016). Getting to the top of mind: How reminders increase saving. *Management Science*, 62(12), 3393–3411. doi:10.1287/mnsc.2015.2296 ✓
- Klasnja, P., et al. (2019). Efficacy of contextually tailored suggestions for physical activity: A micro-randomized optimization trial of HeartSteps. *Annals of Behavioral Medicine*, 53(6), 573–582. ✓
- Koivisto, J., & Hamari, J. (2019). The rise of motivational information systems: A review of gamification research. *Int J Information Management*, 45, 191–210. ✓
- Krebs, P., & Duncan, D. T. (2015). Health app use among US mobile phone owners: A national survey. *JMIR mHealth uHealth*, 3(4):e101. ✓
- Lally, P., van Jaarsveld, C. H. M., Potts, H. W. W., & Wardle, J. (2010). How are habits formed. *European J Social Psychology*, 40(6), 998–1009. doi:10.1002/ejsp.674 ✓
- Lazar, A., Koehler, C., Tanenbaum, J., & Nguyen, D. H. (2015). Why we use and abandon smart devices. *UbiComp '15*, 635–646. doi:10.1145/2750858.2804288 ✓
- Lerner, J. S., & Tetlock, P. E. (1999). Accounting for the effects of accountability. *Psychological Bulletin*, 125(2), 255–275. (mem)
- Li, I., Dey, A., & Forlizzi, J. (2010). A stage-based model of personal informatics systems. *CHI '10*, 557–566. doi:10.1145/1753326.1753409 ✓
- Maier, M., et al. (2022). No evidence for nudging after adjusting for publication bias. *PNAS*, 119(31):e2200300119. ✓
- Mehrotra, A., Pejovic, V., Vermeulen, J., Hendley, R., & Musolesi, M. (2016). My phone and me: Understanding people's receptivity to mobile notifications. *CHI '16*, 1021–1032. doi:10.1145/2858036.2858566 ✓
- Mertens, S., Herberz, M., Hahnel, U. J. J., & Brosch, T. (2022). The effectiveness of nudging: A meta-analysis of choice architecture interventions across behavioral domains. *PNAS*, 119(1). (mem for title; the debate was verified)
- Meyer, R., & Kunreuther, H. (2017). *The Ostrich Paradox: Why We Underprepare for Disasters*. Wharton Digital Press. ✓
- Milkman, K. L., Beshears, J., Choi, J. J., Laibson, D., & Madrian, B. C. (2011). Using implementation intentions prompts to enhance influenza vaccination rates. *PNAS*, 108(26), 10415–10420. ✓
- Milkman, K. L., et al. (2021). Megastudies improve the impact of applied behavioural science. *Nature*, 600, 478–483. ✓
- Milkman, K. L., et al. (2022). A 680,000-person megastudy of nudges to encourage vaccination in pharmacies. *PNAS*, 119(6):e2115126119. ✓
- NFPA (Ahrens, M.). *Smoke Alarms in US Home Fires* (various editions; 2019 edition covering 2012–2016). ✓ (figures via NFPA-derived fact sheets)
- Nunes, J. C., & Drèze, X. (2006). The endowed progress effect. *J Consumer Research*, 32(4), 504–512. ✓
- O'Donoghue, T., & Rabin, M. (1999). Doing it now or later. *American Economic Review*, 89(1), 103–124. ✓
- Peters, G.-J. Y., Ruiter, R. A. C., & Kok, G. (2013). Threatening communication: A critical re-analysis and a revised meta-analytic test of fear appeal theory. *Health Psychology Review*, 7(sup1), S8–S31. doi:10.1080/17437199.2012.703527 ✓
- Rogers, T., & Milkman, K. L. (2016). Reminders through association. *Psychological Science*, 27(7), 973–986. ✓
- Ryan, R. M., & Deci, E. L. (2000). Self-determination theory and the facilitation of intrinsic motivation, social development, and well-being. *American Psychologist*, 55(1), 68–78. (mem)
- Scheibehenne, B., Greifeneder, R., & Todd, P. M. (2010). Can there ever be too many options? A meta-analytic review of choice overload. *J Consumer Research*, 37(3), 409–425. doi:10.1086/651235 ✓
- Silverman, J., & Barasch, A. (2023). On or off track: How (broken) streaks affect consumer decisions. *J Consumer Research*, 49(6), 1095–1117. ✓
- Stawarz, K., Cox, A. L., & Blandford, A. (2015). Beyond self-tracking and reminders: Designing smartphone apps that support habit formation. *CHI '15*, 2653–2662. doi:10.1145/2702123.2702230 ✓
- Tannenbaum, M. B., et al. (2015). Appealing to fear: A meta-analysis of fear appeal effectiveness and theories. *Psychological Bulletin*, 141(6), 1178–1204. ✓
- Thakkar, J., et al. (2016). Mobile telephone text messaging for medication adherence in chronic disease: A meta-analysis. *JAMA Internal Medicine*, 176(3), 340–349. doi:10.1001/jamainternmed.2015.7667 ✓
- Urbach, D. R., et al. (2014). Introduction of surgical safety checklists in Ontario, Canada. *NEJM*, 370, 1029–1038. ✓
- Vance, A., Jenkins, J. L., Anderson, B. B., Bjornn, D. K., & Kirwan, C. B. (2018). *MIS Quarterly*, 42(2). ✓ (venue; title from mem)
- Vervloet, M., et al. (2012). The effectiveness of interventions using electronic reminders to improve adherence to chronic medication. *JAMIA*, 19(5), 696–704. doi:10.1136/amiajnl-2011-000748 ✓
- Vohs, K. D., et al. (2021). A multisite preregistered paradigmatic test of the ego-depletion effect. *Psychological Science*, 32(10), 1566–1581. ✓
- Weber, D., et al. (2018). Snooze! Investigating the user-defined deferral of mobile notifications. *MobileHCI '18*. doi:10.1145/3229434.3229436 ✓

**Industry and grey sources.** These are graded OP or FS and carry conflict-of-interest caveats:

- Nationwide homeowner maintenance survey
- The Hartford home maintenance study
- IBHS plumbing and appliance failure studies (cited secondhand; primary not retrieved)
- LexisNexis Risk Solutions and Flo by Moen water-claims white paper (about 2020)
- Insurance Information Institute home-inventory surveys (2015, 2016)
- UK Fire Kills clock-change survey (Ipsos, 2011–12)
- MPulse and Siemens CMMS blogs on pencil-whipping and PM optimisation
- US Census Bureau Statistical Brief SB/94-07 (1994)
