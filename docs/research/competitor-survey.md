> **Provenance.** Prepared 2026-11-27 by AI research agents (Claude) using web search, for the Housewright design. Sources marked as checked were looked up at the time; anything marked UNVERIFIED or (mem) has not been. Verify a citation before relying on it.

# Competitive survey: home maintenance / home inventory apps (as of 2026-11-27)

Method and caveats. About 30 web searches plus page fetches. Many vendor sites (homebox.software, Centriq help, several App Store pages, GitHub for some repos) returned 403/404 to the fetch tool, so several facts rest on search-result snippets or third-party roundups. Reddit is poorly indexed by the search tool: I found almost no direct Reddit threads, so "user complaints" come mostly from App Store reviews quoted in search results, Capterra/G2 snippets, and Home Assistant forum threads. Anything marked UNVERIFIED rests on a single weak source, my background knowledge, or conflicting sources. Pricing in particular is volatile.

---

## 1. Product profiles

### Group 1: Consumer home maintenance apps

**HomeZada** — https://www.homezada.com
- Platforms: web (secure.homezada.com); mobile apps exist (platform detail UNVERIFIED on the pricing page).
- Open source: no.
- Pricing (homezada.com/homeowners/pricing, fetched): Essentials free (10 AI chats, 5 AI design photos); Premium $118/yr or $21.98/mo; Deluxe $218/yr (2 properties, adds "Home Marketing"); extra properties beyond 2 are $98/yr each; add-ons: $75 to publish a "Zada File", $99 to transfer data to a buyer. Third-party sites give older/conflicting prices ($79/yr, $15.95/mo, $189/yr), which shows how often it changes.
- Sync/hosting: proprietary cloud only.
- Standouts: broadest scope in the group (inventory, maintenance, projects, finances, documents, multiple properties, AI assistant, buyer data transfer). Aimed also at insurance/real-estate use.
- Weaknesses: price rising over time; core value gated behind Premium; AI credit metering; per-property upsell. Few independent reviews (G2 and Capterra show 1 review each), so complaint data is thin.
- Home Assistant: none found.
- Sources: https://www.homezada.com/homeowners/pricing ; https://www.trustradius.com/products/homezada/pricing ; https://softwarefinder.com/property-management-software/homezada

**Centriq** — https://www.centriqhome.com (help: intercom.help/centriq)
- Platforms: iOS, Android, web.
- Open source: no.
- Pricing: was free; moved to a subscription/premium tiers ("add more items" is a paid upgrade). A developer reply in an App Store review said the choice was subscribe-or-shut-down; 2016 coverage had promised "always free".
- Sync: AWS-hosted cloud. Export exists ("Export My Inventory", emailed report incl. model/serial numbers) per help-centre snippets.
- Standouts: photograph the serial plate/barcode and it auto-attaches the manual, parts, videos and recall alerts. Best-known "appliance catalog" model. Also sold to builders/manufacturers (data B2B).
- Weaknesses: wrong manuals matched (user uploaded Whirlpool dryer, got an IKEA dishwasher manual); no keyword search across documents; "item came back blank" is a listed support FAQ; subscription change; aggregator reports 3.5/5 with 53% negative (single source).
- Status: A competitor page (DomiDocs) claims Centriq shuts down on "January 31" with data deleted. I could not fetch it (403), it gives no year in the snippet, and no independent source confirms it. UNVERIFIED. Treat as "possibly sunsetting; check before citing". Related lesson regardless: export-before-shutdown is a recurring theme in this category.
- Home Assistant: none.
- Sources: https://apps.apple.com/us/app/centriq/id1122170439 ; https://justuseapp.com/en/app/1122170439/centriq/reviews ; https://intercom.help/centriq/articles/1236150-can-i-back-up-my-information-from-centriq ; https://domidocs.com/homeowners-guide/centriq-alternative/ (unverified claim)

**Homer** — https://apps.apple.com/us/app/id1250049341 (developer Home Owner AB)
- Platforms: iPhone (iOS 17.6+), also runs on Apple-silicon Mac/Vision; Android not confirmed. 
- Pricing: free download, in-app "Premium" subscription tiers $4.99 to $69.99 plus $9.99 "one more home".
- Features: maintenance and to-dos, inventory, home timeline, expenses, documents (manuals/receipts), AI helper bot, floor-plan scanner, AR notes.
- Weaknesses (reviews): "complex at first"; no warranty tracking yet (planned), no barcode scanner (not planned). 4.3/5, 170 ratings.
- Home Assistant: none. Open source: no. Sync: proprietary cloud (assumed, UNVERIFIED).

**Oply** — https://apps.apple.com/us/app/id6504293662 (developer Myhome Services Inc.)
- iOS (Android UNVERIFIED). Free in the store, premium features gated (price not found). AI-driven "predictive maintenance" reminders, stores warranties/receipts/manuals/service records, pro-service pricing estimates, on-demand pros. ~31 ratings, 4.9 on one tracker. Small sample, review sites are not independent. No HA.

**Dwellin** — https://apps.apple.com/us/app/dwellin-easy-home-maintenance/id1566306121
- iOS (and Android UNVERIFIED). Free; "Dwellin Pro" $2.99/mo or $24.99/yr (search snippet; store page didn't list IAP prices). Builds a home profile from public property data, generates maintenance guidance and calendar, digital home binder (manuals, warranties, contacts), cost tracking, points/rewards programme.
- Weaknesses (App Store): iPad layout cuts off buttons; binder items missing; model/serial text overlaps. 4.4/5, 181 ratings. Developer says tablet UI being improved.
- Interesting angle: pre-population from public property records.

**UpKept (Consumer Reports)** — https://apps.apple.com/us/app/upkept/id1546005008
- iOS + Android. Free (CR launch article says free; a third-party roundup claiming $99.95/yr is unreliable). "Home Hub": add appliances/systems, get plan and reminders; can book pros via Thumbtack. No evidence of shutdown (store pages were not fetchable, so recency UNVERIFIED). No HA, closed source.

**Thumbtack (home care features)** — https://www.thumbtack.com
- iOS/Android/web. Since 2024 "one app for your home": save tasks into a maintenance plan, home profile, acquired Setter (home management); a $49/yr membership was reported in 2024 press (may have changed). Maintenance is a lead-gen funnel into its marketplace, not a standalone tool. No HA.
- Sources: https://www.thumbtack.com/guide/content/the-biggest-product-update-in-our-history ; https://press.thumbtack.com/announcements/thumbtack-debuts-first-ever-home-care-experience-to-partner-with-homeowners-in-the-continuous-care-of-their-largest-asset

**BrightNest** — acquired by Angie's List years ago; all coverage found is 2010-2013 and I found no current listing. Treat as DEFUNCT/UNVERIFIED (could not confirm either way). https://www.loopinsight.com/2013/03/18/home-organization-app-brightnest/

**HomeKeepr** — https://apps.apple.com/us/app/id6751008561 
- Originally (2015-16) an agent-branded post-closing app (Inman coverage, ~$9.99 for 100 clients); the current store listing is a consumer "digital home manual" with a paid tier adding warranty/service tracking. Several unrelated apps share near-identical names (HomeKeep, HomeKeep+, HomeKeepr Pro). Agent portal today: UNVERIFIED.
- https://inman.com/2015/04/24/homekeepr-keeps-agents-in-touch-with-clients-after-the-close-but-for-how-long

**Newer entrants (2025-2026), all UNVERIFIED beyond store listings** — HomeGrove, Hearth, Upkeepy, Homies, HomeKeep, Manor, Upmend, Handled, Houseminder, Tiny Maintenance, FixNLog, Home Maint Log. Dozens of small iOS-only indie apps with very similar pitches ("appliance log + reminders"), mostly subscription. Evidence of a crowded low-end, not of a clear leader. Example listing: https://apps.apple.com/us/app/homegrove-home-maintenance/id6759873481

### Group 2: Open source / self-hosted

**Homebox (SysAdminsMedia continuation)** — https://github.com/sysadminsmedia/homebox (docs: homebox.software, mirror https://321057ad.homebox-docs.pages.dev/en/user-guide/notifiers)
- Platforms: web (responsive) only; no official native mobile app found. Docker self-host.
- License: AGPL-3.0 per several secondary sources; the GitHub page fetch did not name it (verify LICENSE file). The original hay-kot repo was the upstream; sysadminsmedia is the active continuation (~8k stars on the fetched page).
- Pricing: free; no official paid tier found.
- Features: locations (nested), labels/tags, items with purchase info, warranty, attachments/photos, QR labels, CSV import/export, multi-user groups, scheduled maintenance entries; daily check sends reminders through Shoutrrr notifiers (18+ services).
- Weaknesses: inventory-first, maintenance is a per-item log with dates, no template library, no floor plans, no barcode scan (QR only per one comparison), no native mobile app. One comparison table marks maintenance logs "No" (conflict; the notifier docs say yes).
- Home Assistant: community HACS integration "HomeBox" v0.1.0 (sensors for totals/value, link HA devices to items, sync HA areas to locations); does not mention maintenance entries. https://community.home-assistant.io/t/release-homebox-custom-integration-for-home-assistant-hacs/995954

**Grocy** — https://grocy.info , https://github.com/grocy/grocy
- Self-hosted PHP web app, Android app (barcode scanning); MIT (background knowledge, UNVERIFIED this session).
- Chores module: recurring chores with a Period Type, tracks who did it and when, user assignment; separate one-off Tasks; Batteries module; Equipment module (stores manuals; I found no equipment entity in the HA integration). Multi-user supported.
- Weaknesses: setup effort, designed around consumables; no warranty tracking, no QR labels; chore period options and "yearly" had HA integration errors in an old forum post.
- Home Assistant: yes, HACS integration custom-components/grocy: binary sensors for overdue chores/tasks/batteries/products, sensors for chores, tasks, batteries, stock, shopping list, meal plan; entities are disabled by default. https://github.com/custom-components/grocy ; https://github.com/grocy/docs/blob/master/tutorials/chores.md
- Reddit r/grocy thread "Using chores for maintenance" exists (Mar 2025), confirming people stretch chores into maintenance: https://reddit.birdcat.cafe/r/grocy/comments/1jb7bma/using_chores_for_maintenance

**Snipe-IT** — https://github.com/snipe/snipe-it : AGPL-3.0 (fetched), ~15k stars, self-host PHP, hosted version exists (mobile app not compatible with hosted version per README). IT-asset oriented (check-in/out, kits, licences); it has an "asset maintenances" log (background knowledge; README does not mention it; one vendor says it is a record, not a scheduler). Not designed for homes. No HA integration found.

**InvenTree** — https://github.com/inventree/InvenTree : MIT (fetched), ~8k stars, Python/Django, REST API, plugins, React UI, companion mobile app. Parts/stock/BOM oriented; no maintenance scheduling found. Heavy for home use. No HA integration found.

**LubeLogger** — https://github.com/hargata/lubelog , docs https://docs.lubelogger.com
- Vehicle maintenance, MIT per directory listings (one fork page mentions dual licensing: verify). Self-host (.NET), managed hosts exist (ElfHosted from ~$1.29/mo, beta). Reminders by odometer and date, pushable via ntfy etc. Not a house tool: a good model for odometer/usage-based reminders and "log everything" UX. HA integration: UNVERIFIED (none found).
- Notable: a forum poster asked for "LubeLogger for the house"; replies pointed to Grocy/Homebox. That is the gap this new app fills. https://hackaday.com/2025/03/28/keep-tabs-on-your-vehicles-needs-with-lubelogger

**Maintainerr** — unrelated (Plex/Jellyfin media cleanup tool); not surveyed.

**Home Assistant HACS maintenance integrations** (all custom, none is HA core): see section 6 for the data models.
- Home Maintenance (TJPoorman) — https://github.com/TJPoorman/home_maintenance — MIT, ~95 stars.
- Maintenance Supporter (iluebbe) — https://github.com/iluebbe/maintenance_supporter — MIT; now in the HACS default store (June 2026); v2.18 by July 2026.
- Home Keeper (prestomation) — https://community.home-assistant.io/t/home-keeper-extensible-home-maintenance-tracking/1014673 — license not stated in post.
- Chore Helper (bmcclure) — https://github.com/bmcclure/ha-chore-helper — scheduling by "every N" or "after N" completion; v0.5.0 latest seen.
- Also: Battery Notes (https://github.com/andrew-codechimp/HA-Battery-Notes), Home Tasks (L3t4l3s/home-tasks), maint, Chore Calendar, Smart Device Maintenance Manager, Device Maintenance Monitor, routinely, KidsChores. The crowd of near-duplicates is itself a finding: demand is high, no single winner, everybody re-invents the data model.

### Group 3: Landlord / property-management / CMMS (maintenance features only)

**Landlord Studio** — https://www.landlordstudio.com/features/property-maintenance
- iOS/Android/web. Free "Go" plan for up to 3 units; paid from about $12/mo for 3+ properties (Capterra UK snippet). Maintenance: tenants submit requests with up to 5 photos, status New to Completed. Reviews call maintenance the weak spot (older TurboTenant review: no comments on requests, no contractor assignment, no job costs; may be fixed). Praised support. No HA.

**Buildium** — https://www.buildium.com : from about $62/mo (Software Advice). Work orders, recurring items, vendor and owner portals. Complaints (G2/Capterra snippets): minimal work-order functions, work orders not linked to tenant accounts, poor support, payment fee opacity. No HA.

**TenantCloud** — https://www.tenantcloud.com/maintenance : Starter about $15-18/mo; 14-day trial (free-plan claims conflict). Unlimited online requests, recurring requests, ServicePro vendor portal, auto-assign on higher tiers. Complaints: payment handling, Trustpilot 3.3; no automation to assign vendors to recurring tasks.

**Breezeway** — https://www.breezeway.io : vacation-rental ops; auto-generates cleaning/inspection/maintenance tasks from reservations, checklists and photos, assignments. Pricing opaque (listings range from $4-6 per user per month to "contact sales"). Reviewers want subtasks, task search, linked maintenance items, better mobile. No HA.

**UpKeep** — https://upkeep.com : CMMS, no permanent free tier (trial only); about $20-55/user/mo depending on source (conflicting). Preventive-maintenance schedules (time and meter), work orders, assets, parts. Per-seat pricing is the barrier for households.

**MaintainX** — https://www.getmaintainx.com : permanent free tier (reported tight caps, UNVERIFIED), paid about $20-65/user/mo (conflicting sources). Checklists/procedures, PM, work orders. One source claims an Autodesk acquisition in Aug 2026: UNVERIFIED. Sources: https://reliamag.com/guides/upkeep-vs-maintainx/ ; https://tractian.com/en/blog/maintainx-vs-upkeep

None of Group 3 integrates with Home Assistant (UNVERIFIED but none found).

---

## 2. Feature matrix

Legend: Y = confirmed in sources; p = partial/limited; - = not found/absent; ? = unverified. "Templates" means pre-built per-appliance maintenance plans.

| Product | Recurring reminders | Inventory | Docs/manuals/receipts | Floor plan | Generic templates | Make/model data | Audit log | Photos | Contractors | Multi-property | Multi-user | Usage/season triggers | Offline/local | Open source | HA |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| HomeZada | Y | Y | Y | ? | Y (checklists) | p | p | Y | Y | Y (paid) | p | - | - | - | - |
| Centriq | Y | Y | Y (auto manual) | - | - | Y (core) | p | Y | p | ? | p | - | - | - | - |
| Homer | Y | Y | Y | Y (scanner) | ? | - | Y (timeline) | Y | ? | Y (add-on) | ? | - | - | - | - |
| Oply | Y (predictive) | Y | Y | - | ? | ? | Y | ? | Y (marketplace) | ? | ? | ? | - | - | - |
| Dwellin | Y | Y | Y | - | Y (generated) | - | Y | ? | Y | ? | ? | - | - | - | - |
| UpKept | Y | Y | ? | - | Y | ? | ? | ? | Y (Thumbtack) | - | - | - | - | - | - |
| Thumbtack | p | p | - | - | Y | - | - | - | Y | - | - | - | - | - | - |
| Homebox | Y (dates) | Y | Y | - | - | - | Y (per-item) | Y | - | p (groups) | Y | - | Y (self-host) | Y AGPL | p (community) |
| Grocy | Y (chores) | Y | p | - | - | - | Y (who/when) | p | - | - | Y | - | Y | Y (MIT?) | Y (community) |
| Snipe-IT | p | Y | Y | - | - | - | Y | Y | - | Y | Y | - | Y | Y AGPL | - |
| InvenTree | - | Y | Y | - | - | - | Y | Y | - | - | Y | - | Y | Y MIT | - |
| LubeLogger | Y (date+odometer) | Y (vehicles) | Y | - | - | - | Y | Y | - | - | Y | Y (odometer) | Y | Y MIT | ? |
| HA Home Maintenance | Y | - | - | - | - | - | p | - | - | - | HA users | - | Y (local) | Y MIT | Y (is HA) |
| HA Maintenance Supporter | Y | p (objects) | p | - | p (suggested setups) | - | Y (cost, notes, photos) | Y | - | - | Y | Y (strongest) | Y | Y MIT | Y |
| HA Home Keeper | Y (4 task types) | p (appliances) | ? | - | - | - | Y (note, cost, photo, who) | Y | - | - | Y | p (triggered) | Y | ? | Y |
| Landlord Studio | p | p | Y | - | - | - | Y | Y | p | Y | Y (tenants) | - | - | - | - |
| Buildium | Y (recurring) | p | Y | - | - | - | Y | Y | Y | Y | Y | - | - | - | - |
| TenantCloud | Y (recurring) | p | Y | - | - | - | Y | Y | Y (portal) | Y | Y | - | - | - | - |
| Breezeway | Y (generated) | p | - | - | Y (checklists) | - | Y | Y | Y | Y | Y | event-based | - | - | - |
| UpKeep / MaintainX | Y (time+meter) | Y (assets) | Y | p | Y | - | Y | Y | Y (vendors) | Y | Y (seats) | Y (meter) | p (offline mobile) | - | - |

Cells are drawn from the profiles above; "?" means I did not find evidence either way.

---

## 3. Table stakes (nearly every competitor has these)

- Recurring reminders with push notification (fixed interval; "after completion" vs "calendar fixed" is the main differentiator).
- Inventory/asset list with model, serial, purchase date, photos.
- Attachments for manuals, receipts, warranties.
- Per-item service history/log.
- Categories/rooms/locations (Homebox: nested locations; Centriq/Dwellin: rooms).
- Export (CSV or report) of at least the inventory (Centriq, Homebox, Home Keeper CSV "for insurance").
- Quick "mark done" on the reminder.

## 4. Rare but loved

- Photograph the label and get the manual and parts automatically (Centriq). Most-praised single feature in reviews; also the one with accuracy complaints.
- Usage-based and sensor-triggered tasks (Maintenance Supporter, LubeLogger odometer, CMMS meters). Loved by the HA crowd; almost absent from consumer apps.
- Four task kinds (floating, fixed, one-off, triggered) in one engine (Home Keeper).
- NFC tag on the appliance that completes the task when scanned (Home Maintenance, Maintenance Supporter; both have NFC-dropdown bugs).
- Prepopulating the home from public property records (Dwellin).
- Floor-plan scanning and AR notes (Homer, iOS only).
- Seasonal windows, vacation mode, household task rotation (Maintenance Supporter).
- Adaptive intervals learned from history (Maintenance Supporter, optional).
- Home "transfer to buyer" data package (HomeZada, charged $99) and CSV for insurance (Home Keeper).
- Spare-parts and filter stock linked to tasks (Maintenance Supporter, Home Keeper).
- Voice/Assist intents for complete/postpone (Maintenance Supporter, 15 intents).
- Free multi-user without per-seat pricing (Homebox, Grocy) vs per-seat CMMS.

## 5. Recurring complaints and exploitable gaps

1. **Subscription creep and paywalling.** Centriq moved from "always free" to subscription; HomeZada pricing has moved repeatedly and meters AI usage and properties; dozens of indie apps are subscription-only. A one-time-purchase or free local-only core with paid optional sync addresses this directly.
2. **Lock-in and shutdown risk.** Centriq's unverified-shutdown rumour, BrightNest apparently gone, Belkin Wemo (Jan 2026) as category backdrop. Strong export, open file format, and local-first is a differentiator. Centriq's export is a report, not a re-importable format (as far as help snippets show).
3. **Wrong or missing manuals/templates.** Centriq's mis-matched manuals; Dwellin "items missing from binder". Opportunity: transparent confidence ("matched by model number" vs "guess"), user-correctable, community-editable templates.
4. **Weak multi-user and multi-property.** HomeZada caps at 2 properties on Deluxe then charges per property; Landlord Studio lacks contractor assignment and job costing (older review); Buildium work orders not linked to tenant accounts. Household assignment (who did it) mostly exists only in Grocy/HA tools.
5. **Landlord tools are wrong-sized for households, CMMS per-seat pricing is wrong for families.** UpKeep/MaintainX/Buildium priced per user or per unit. A single tool spanning first-time owner to small landlord is unserved.
6. **Self-hosted tools stop at inventory.** Homebox has no templates or plans; Grocy needs heavy setup and is food-centric; people explicitly ask for "LubeLogger for the house".
7. **HA ecosystem fragmentation.** At least 8-10 overlapping HACS integrations, each with its own schema, UI panel, and quirks; users hand-build dashboards with flex-table/auto-entities. No integration has inventory + manuals + floor plans + templates. An app that exposes a stable HA API (todo/calendar/sensor entities, events) would sit above this.
8. **iPad/tablet and cross-platform polish.** Homer is iPhone-only; Dwellin iPad layout broken; Homebox has no native mobile app; most indie apps are iOS-only. Android + iOS + web with offline use is rare.
9. **UX gripes seen in HA threads:** fixed US date format, cannot sort or search tasks, tag/NFC management confusion, no overdue-count entity, restart breaks NFC names. Cheap wins.
10. **Notification reliability.** Maintenance Supporter users initially saw no notifications; Homebox notification troubleshooting is a documented section. Reminders are the core promise, so test them hard.

## 6. Data sources for make/model maintenance data and manuals

I found no open dataset of maintenance plans keyed by make/model. The new app will likely need to build and curate its own (a community-contributed template repo), seeded from public sources below.

- **ENERGY STAR certified product data** — data.energystar.gov (Socrata/SODA API; ~40-50+ datasets updated daily; includes brand, model name/number, and specs for water heaters, heat pumps, central AC, thermostats, washers, etc.). Listed under an "EPA Data License" on data.gov; I could not read the license text, so verify before redistribution. The ENERGY STAR mark itself is a trademark that needs EPA permission to use. Good for model-number validation and device-class specs, not maintenance steps. https://www.energystar.gov/products/spec/energy_star_api_user_essentials_pd ; https://catalog.data.gov/dataset/energy-star-certified-water-heaters
- **CPSC Recalls API** — https://www.saferproducts.gov/RestWebServices/Recall (XML/JSON, free; data.gov lists a public-domain label; programmers guide v1.4, 2018, so check endpoints). Useful for recall alerts on user inventory. https://www.cpsc.gov/node/5808
- **ManualsLib** and similar aggregators (manuals.plus, manualsnet.com) — user-upload/aggregated copyrighted manuals with DMCA takedown processes; I could not find a public API or a licence permitting bulk reuse. Do not scrape or redistribute; link out or let users attach their own PDFs. https://www.manualslib.com/takedown.html
- **Manufacturer manual sites** (e.g. GE Appliances support, BSH/Siemens, AEG, V-ZUG) — free to download for end users; terms vary and typically forbid redistribution. Best approach: store a link plus a user-fetched local copy. Manufacturer APIs for manuals/maintenance: none found publicly (UNVERIFIED).
- **Serial-number date decoding** — Homespy.io offers a token-authenticated commercial API returning an estimated manufacture year, with caveats that serials are ambiguous across decades. No open-source multi-brand decoder found; GE publishes its own scheme. https://homespy.io/documentation ; https://products.geappliances.com/appliance/gea-support-search-content?contentId=37870
- **Open datasets for manuals:** none found; one academic dataset on maintenance needs of home appliances (University of Coimbra) exists but I did not open it (UNVERIFIED relevance/licence): https://estudogeral.uc.pt/bitstream/10316/113010/1/Dataset%20for%20identifying%20maintenance%20needs%20of%20home%20appliances%20using%20artificial%20intelligence.pdf
- **Battery Notes library** (HA) — an open community library mapping device models to battery types; a good precedent for a crowd-curated model library. https://github.com/andrew-codechimp/HA-Battery-Notes
- **Not checked (suggested follow-up, UNVERIFIED):** Open Products Facts / Wikidata / GS1 (my search for these failed), AHRI directory (HVAC models), AHAM, Whirlpool/Rheem/Bradford White published maintenance guides, Wikipedia-style generic maintenance intervals (e.g. from InterNACHI, NAHB, FEMA/HUD home-maintenance checklists, which are generally public or public domain; check each).

## 7. How HA integrations model maintenance

| Integration | Unit of record | Schedule types | Completion | HA entities / hooks |
|---|---|---|---|---|
| Home Maintenance (TJPoorman, MIT) | Task: title, interval, interval period (days/weeks/months), last_performed, optional NFC tag, icon | Interval from last performed only | `reset_last_performed` service (optional date); Complete button; NFC scan | One `binary_sensor` per task (on = due), attrs `last_performed`, `next_due`; stored in HA `.storage`; sidebar panel |
| Chore Helper (bmcclure) | Chore helper entity | "every N days/weeks/months/years" (from start date), "after N ..." (from completion), manual via service | service call | Sensor with next due date/state; scheduling flexibility is the draw; reports of no future dates in some setups |
| Maintenance Supporter (iluebbe, MIT) | Object (nestable: car inside "Vehicles") has many Tasks; task has priority, labels, assignee, notes, spare-part links; History stores cost, duration, notes, photos | Interval; calendar patterns (nth weekday, day of month, last business day, offsets); events from an HA calendar entity; one-time; manual; reading (meter values); seasonal windows; finite series; postpone | Complete / skip / reset buttons and services; can auto-complete when a sensor recovers | Per-task sensor (ok / due soon / overdue / triggered / paused / archived), problem binary sensor, optional next-due and days-until sensors, global summary sensors, stock sensors, calendar + to-do entity, lifecycle events, 14 services, 15 Assist intents. Triggers: threshold, counter, runtime, state-change cycles, AND/OR compounds. Optional adaptive intervals (EWA/Weibull) |
| Home Keeper (prestomation) | Task and separate Appliance (device page, metadata, wear items, spare parts); labels and areas | Floating (N after last completion), Fixed (calendar), One-off (dormant after completion), Triggered (armed/cleared by another integration e.g. Battery Notes) | One-tap Done; note, cost, photo, who | Sidebar panel, card, to-do and calendar entities, per-device sensors, `home_keeper.*` services accepting id or name, events as device triggers, CSV export; 16 languages |
| Grocy HA integration | Chores/tasks/batteries come from Grocy server | Grocy period types | Done in Grocy (or service) | binary sensors (overdue chores/tasks/batteries), sensors; entities disabled by default |

Take-aways for the schema:
- "Floating" (interval after completion) versus "fixed" (calendar) versus "triggered" (external or sensor) versus "one-off" is the emerging common vocabulary. Support all four.
- Separate "thing being maintained" (object/appliance, nestable) from "task", and store completions as a history table with cost/note/photo/who: three of four integrations converged on this.
- Expose states (ok, due soon, overdue, triggered, paused) not just a boolean; add days-until-due and an overdue count (users keep asking for both).
- Provide HA-native surfaces: to-do and calendar entities, device triggers/events, services by name, Assist intents, NFC completion, and a template to adopt existing sensors (battery low, `device_class: problem`).
- Known pain points from threads: no sort/search, locale date format, tag/NFC UI, dashboard-table workarounds, notification latency.

---

## 8. Quick design implications (opinion, from the above)

1. Win on trust: local-only default, open file format with lossless export/import, no feature paywall on the core loop; charge for hosted sync only.
2. Templates are the moat: a community-curated, versioned, licence-clean template repo (generic first, make/model second), with provenance and "confidence" shown, because Centriq's mismatched manuals were the most visible failure.
3. Model four schedule kinds plus usage meters from day one; and make Home Assistant an adapter on the same engine (todo/calendar/sensor entities, events) rather than a separate product.
4. Multi-property and multi-person must be free of per-seat/per-property penalties to differentiate from HomeZada and CMMS pricing; keep a "tenant request to work order" path lightweight so landlords are served without becoming Buildium.
5. Link rather than host copyrighted manuals; let users attach their own PDFs; use CPSC recalls and ENERGY STAR data (after checking licences) as free enrichment.
