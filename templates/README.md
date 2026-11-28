# Maintenance templates

Templates describe a kind of thing in a house (a smoke alarm, a water heater)
and the maintenance it needs. The app ships a copy of this folder and turns a
template into suggested tasks when you add an item (D-21).

Every file here is written in our own words from widely published public
guidance. **Do not copy manufacturer manuals or other people's text** into a
template, and do not invent statistics. If you want to cite something, name the
*kind* of source in `source` (for example "NFPA guidance" or "common
manufacturer guidance"); a manual can be attached to an item by its owner but is
never redistributed here.

The test `crates/core/tests/templates.rs` loads and validates every file in this
folder, so `cargo test` tells you if a template is wrong.

## File format (version 1)

One template per file, in [TOML](https://toml.io). The file name is the
template id plus `.toml`.

```toml
format = 1                    # always 1 for now
id = "water-heater"           # lowercase letters, digits, hyphens; = file name
name = "Tank water heater"    # what people see
kind = "generic"              # "generic" or "model"
category = "plumbing"         # optional; inherited if missing
description = "..."           # optional

# For a specific make and model:
# kind = "model"
# extends = "water-heater"    # required for models: the parent template id
# make = "Acme"
# model = "WH50"

[[fields]]                    # facts recorded about the item (D-22)
key = "fuel"                  # lowercase letters, digits, underscores
label = "Fuel"
type = "choice"               # text | number | date | yes_no | choice | photo
options = ["gas", "electric"] # choice only
# unit = "gal"                # number only, optional
# sensitivity = "sensitive"   # "normal" (default) or "sensitive": sealed binder (D-41)
# help = "..."                # optional

[[meters]]                    # usage counters, for usage schedules
key = "loads"
name = "Loads washed"
unit = "loads"

[[tasks]]
key = "flush"                 # unique within the template
title = "Drain and flush the water heater"
priority = "efficiency"       # life_safety | property_loss | efficiency | cosmetic
schedule = { kind = "fixed", every = "1 year" }
minutes = 60                  # optional: how long it takes
steps = ["...", "...", "..."] # 3 to 7 short steps
why = "One line: why it matters."   # at most 200 characters
source = "common manufacturer guidance"
when = { field = "fuel", equals = "gas" }   # optional: only for matching items
window = { lead_days = 14, grace_days = 30 } # optional: override the due window
completion_fields = [         # optional: offered after "Done", never required
  { key = "result", label = "Result", type = "choice", options = ["pass", "fail"] },
]
```

### Schedules

| `kind` | Other keys | Meaning |
|---|---|---|
| `fixed` | `every = "3 months"` | Every N days/weeks/months/years, counted from when the task starts, whenever it was last done. |
| `nth_weekday` | `nth = 2`, `weekday = "saturday"`, `months = [3, 10]` (optional) | The nth (1-4, or -1 for last) weekday of the month, in the listed months or every month. |
| `months_of_year` | `months = [4, 10]`, `day = 1` (optional) | Specific months, on a day of the month (clamped to the month's length). |
| `floating` | `every = "90 days"` | N after it was last done. |
| `seasonal` | `season = "before_first_frost"` | Once a year in a window: `spring`, `summer`, `autumn` (or `fall`), `winter`, `before_first_frost`, `after_last_frost`. Resolved from the property's hemisphere and frost dates. |
| `usage` | `meter = "loads"`, `threshold = 40.0`, `or_every = "2 months"` (optional) | When the item's meter has advanced by the threshold since it was last done, or after `or_every`, whichever is first. The meter must be declared in `[[meters]]`. |

Intervals are written as a whole number and a unit: `"1 day"`, `"2 weeks"`,
`"3 months"`, `"10 years"`. Templates cannot contain one-off tasks or fixed
anchor dates; those belong to a particular house.

### Conditions

`when = { field = "...", equals = "..." }` includes a task only for items whose
field has that value. For a `choice` field, `equals` must be one of its options;
for a `yes_no` field, `"yes"` or `"no"`. An item with the field unset does not
get the task.

### Inheritance

A model template `extends` a generic one. The resolved template has the parent's
fields, meters and tasks, with the child's entries replacing any with the same
`key` and adding new ones. Chains (model -> generic -> category) are allowed;
cycles are an error.

## The library

| File | Covers |
|---|---|
| **Safety** | |
| `smoke-alarm.toml` | Monthly test, battery, cleaning, ten-year replacement |
| `co-alarm.toml` | Monthly test, battery, end-of-life replacement |
| `fire-extinguisher.toml` | Monthly gauge and seal check, show everyone, replace per the label |
| **Heating and cooling** | |
| `hvac-air-filter.toml` | Furnace / HVAC filter |
| `furnace.toml` | Book yearly service (gas, oil), check-up (electric), autumn start-up, vent check, oil tank |
| `boiler.toml` | Book yearly service by fuel, pressure gauge, bleed radiators (hot water), leaks |
| `heat-pump.toml` | Clear outdoor unit, book yearly service, condensate drain |
| `central-ac.toml` | Clean outdoor condenser, condensate drain, book pre-season service |
| `window-ac.toml` | Wash filter in season, clean coils, check it is secure, winter storage |
| `fireplace.toml` | Book yearly chimney inspection, damper and screen, safe ash disposal |
| `wood-stove.toml` | Book yearly inspection, door gasket and stovepipe, safe ash disposal |
| `dehumidifier.toml` | Filter, bucket or drain hose, coils and humidity setting |
| **Plumbing** | |
| `water-heater.toml` | Flush, relief valve, anode, gas flue (gas only) |
| `tankless-water-heater.toml` | Descale, inlet filter, book vent check (gas only) |
| `heat-pump-water-heater.toml` | Air filter, condensate drain, relief valve, flush |
| `well-pump.toml` | Yearly water test, pressure tank, wellhead, book inspection |
| `septic-system.toml` | Book inspection and pumping, drain field walk, water habits |
| `water-softener.toml` | Salt level and bridges, brine tank clean |
| `whole-house-water-filter.toml` | Cartridge replacement, housing O-ring |
| `main-water-shutoff.toml` | Exercise the valve, label it and show everyone |
| `toilets.toml` | Dye test, fill and shutoff valves, base |
| `under-sink-plumbing.toml` | Look for leaks, supply lines and shutoff valves |
| `tub-shower-caulk.toml` | Inspect caulk and grout, recaulk |
| `sump-pump.toml` | Test, spring pit clean, backup battery (if fitted) |
| **Appliances** | |
| `washing-machine.toml` | Supply hoses, cleaning cycle (usage-based) |
| `clothes-dryer.toml` | Lint screen, vent duct, outside flap |
| `refrigerator.toml` | Coils, water filter (if fitted), door seals |
| `dishwasher.toml` | Filter, spray arms, hoses and leaks |
| `garbage-disposal.toml` | Safe cleaning, what not to put down it |
| `range-hood.toml` | Degrease filter, charcoal filter (if recirculating), fan and light |
| **Electrical** | |
| `electrical-panel.toml` | GFCI test, AFCI test (if fitted), panel clear and labelled, book inspection |
| `generator.toml` | Standby: exercise check, book service; portable: safe setup and fuel |
| `solar-panels.toml` | Monitoring check, look from the ground, book inspection |
| **Exterior** | |
| `gutters.toml` | Autumn clean, spring check |
| `hose-bibs.toml` | Winterise before first frost, turn on after last frost |
| `garage-door-opener.toml` | Safety reverse test, lubricate, book spring check |
| `exterior-siding.toml` | Spring and autumn walk-round, repaint or repair |
| `deck.toml` | Spring inspection, reseal (wood), clean (composite) |
| `windows-doors.toml` | Weatherstripping, locks, escape windows |
| **Structure** | |
| `roof.toml` | Look from the ground spring and autumn, book inspection |
| `attic.toml` | Autumn inspection, check after heavy rain |
| `crawlspace.toml` | Inspection, vents and access (if vented) |
| **Yard** | |
| `lawn-mower.toml` | Blade, oil (gas), clean and store for winter |
| `irrigation.toml` | Spring start-up, winterise, book backflow test |
| `pool.toml` | Weekly water test, filter, fence and gate, open or close for the season |

### Categories

`category` must be one of: `safety`, `heating_cooling`, `plumbing`,
`appliances`, `electrical`, `exterior`, `structure`, `yard` (D-68). The test
`template_quality` in `crates/core/tests/templates.rs` holds the list and the
labels people see; `app/src/lib/labels.ts` must match it.

### Quality rules (checked by the test)

- 3 to 7 steps, each a real sentence; the critical safety step first
  ("Switch off power at the breaker", "Unplug it").
- `why` is one sentence of at most 200 characters; `source` names the kind
  of source. No URLs, no percentages, no invented statistics or part numbers.
- An interval between a week and ten years, and `minutes`.
- A job that is risky for a non-professional (gas appliances, panel
  internals, roofs, chimneys, garage door springs, septic pumping) is a
  reminder to book: the title starts with "Book" and the first step says whom
  to hire.

### Intervals that are judgement, not a named guideline

These were set by the template author from general practice and are worth a
second look by someone with trade experience: electric furnace and boiler
check-up (2 years), garage door spring check (2 years), professional roof
inspection (3 years), recaulking (3 years), solar inspection (5 years), well
inspection (5 years), exterior repaint (7 years), deck reseal (2 years),
electrical inspection (10 years), fire extinguisher replacement (10 years,
steps defer to the label), fireplace ash clear-out (monthly) and wood stove
ash clear-out (2 weeks) which depend on use, furnace vent checks (6 months),
whole-house filter cartridge (6 months; steps defer to the maker). The window
AC filter uses fixed northern-hemisphere months (June to September), and pool
water testing is weekly all year with a first step to skip it while the pool
is closed, because the format has no "weekly in season" schedule yet.

Template text licensing is an open question for the owner (Q-02; proposed
CC BY 4.0).
