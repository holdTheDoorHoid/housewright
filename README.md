# Housewright

An open source home maintenance app for Android, iOS and the web.

Housewright reminds you when the furnace filter, smoke alarms, water heater and
gutters need attention. It keeps the manuals, warranties and receipts for the
things in your house in one place, and records who did what, when, at what cost,
and with which contractor. It prints a house manual you can keep in a binder, so
what you know about your home isn't lost if you move, sell, or aren't around to
explain it.

> **Status: early design and core development.** Nothing is installable yet.
> See the [roadmap](docs/ROADMAP.md).

## What it will do

- **Reminders that fit how houses work:** fixed dates, "90 days after last time",
  seasonal ("before first frost", by zip code), and usage-based ("after 300 hours
  of furnace runtime").
- **One tap to log a task**, from the notification or from a QR sticker on the
  appliance. Notes, photos and costs are optional.
- **An inventory** of appliances and systems, grouped by room and type, with
  manuals, warranties, custom fields and photos.
- **Templates** for common things (a washing machine) and specific models, with
  recommended maintenance plans, from an open community library.
- **A short questionnaire** that starts you on the few tasks that matter most and
  grows the plan from there.
- **Floor plans**, from simple room layouts up to measured plans.
- **Contractors**: your plumber, electrician and roofer, their job history and
  costs, licence and insurance dates.
- **A printable house binder**, with an emergency first page, a handover edition
  for buyers, and a separate sealed volume for codes and accounts.
- **Home Assistant**: tasks show up in Home Assistant, and sensors there can
  trigger tasks.
- **Several people and several houses**, with roles for household members,
  helpers, contractors and tenants.

## Your data

Housewright is **local-first**. It works on one device with no account and no
server. If you want your devices to sync, run your own server (free) or use the
hosted service (paid, planned). Hosted data is **end-to-end encrypted**: the
server stores only data it cannot read.

## Documents

- [Decision log](docs/DECISIONS.md): what we decided and why
- [Design principles](docs/DESIGN-PRINCIPLES.md): how we keep people using it
- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md)
- Research: [competitor survey](docs/research/competitor-survey.md),
  [human factors review](docs/research/human-factors-review.md)

## License

[AGPL-3.0](LICENSE). You may use, change and self-host Housewright freely. If you
run a modified version as a service for others, you must publish your changes.
