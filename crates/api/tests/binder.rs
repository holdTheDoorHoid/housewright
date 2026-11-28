//! The house binder end to end (Phase 5): a realistic household full of
//! sensitive values, every edition built and typeset, and the checks that
//! matter most: nothing sensitive in a main volume, the private rating
//! never printed, the handover edition anonymised, the emergency page on
//! one page, user text never read as Typst markup, and change tracking.
//!
//! `HW_BINDER_OUT=dir cargo test -p housewright-api --test binder` also
//! writes every PDF to `dir`, to look at.

use std::io::Cursor;

use housewright_api::Api;
use housewright_binder::{self as binder, Edition, Options, Paper};
use housewright_core::blobs::SqliteBlobs;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::store::{ManualClock, Store};
use serde_json::{Value, json};

/// 2026-11-27 12:00 UTC.
const NOW_MS: u64 = 1_795_780_800_000;
const DAY_MS: i64 = 86_400_000;

/// Every sensitive value in the demo household. None may appear in a
/// main volume.
const SECRETS: &[&str] = &[
    "LarchNet-5G / correct-horse-battery-9",
    "4417#",
    "PECO 8812-44-1907",
    "Gas acct 55-01-7781",
    "Under the third flowerpot left of the back door",
    "Grandma's ring is in the blue tin in the pantry",
    "Bedroom closet, under the loose floorboard",
    "SAFE-SN-00912",
    "POL-99-1234-X",
    "Overcharged us in 2024, get a second quote",
    "HVAC-LIC-778812",
    "Gun safe combination 31-07-22",
    "Heirloom silver",
];

/// A contractor's private notes: never printed, never in an export for others.
const PRIVATE_NOTE: &str = "PRIVATE-NOTE: slow to call back, fair prices";

struct Demo {
    api: Api,
    owner: String,
    member: MemberId,
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn photo(shade: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(1200, 900, |x, y| {
        let band = ((x / 60 + y / 60) % 2) as u8;
        image::Rgb([shade, 120 + band * 60, 200 - (y % 200) as u8])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn item_id(api: &mut Api, template: &str) -> String {
    let rows = call(api, "items", Value::Null);
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["template"] == template)
        .unwrap_or_else(|| panic!("no {template}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn fact(api: &mut Api, group: &str, label: &str, value: &str, key: Option<&str>) -> String {
    call(
        api,
        "save_fact",
        json!({"group": group, "label": label, "value": value, "key": key}),
    )["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn demo() -> Demo {
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut api = Api::with_blobs(store, Box::new(SqliteBlobs::in_memory().unwrap())).unwrap();
    call(
        &mut api,
        "init",
        json!({"household_name": "The Riveras", "property_name": "14 Larch Lane",
               "your_name": "Morgan Rivera", "time_zone": "America/New_York"}),
    );
    call(
        &mut api,
        "onboard",
        json!({"profile": {
            "home_type": "house", "built": "1980_1999", "zip": "19119",
            "heating": ["gas_furnace"], "cooling": ["central_ac"],
            "water_heater": "tank_gas", "water_supply": "city", "sewer": "city",
            "features": {"basement": true, "sump_pump": true, "dryer": true, "washer": true,
                          "dishwasher": true, "garage": true, "gutters": true, "attic": true},
            "smoke_alarms": "two_to_three", "smoke_power": "battery", "co_alarms": "one",
            "people": {"adults": 2, "children": 1}
        }}),
    );
    let owner = call(&mut api, "members", Value::Null)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // A second person, whose name must not reach the handover edition.
    let member = MemberId::new();
    {
        let s = api.store_mut();
        s.put(&Member {
            id: member,
            name: "Sam Rivera".into(),
            role: Role::Member,
            kind: Default::default(),
            contact: Some(Flagged::sensitive("sam@example.com".into())),
            reminders: None,
        })
        .unwrap();
        // Switch on every task, so the plan is full.
        for mut t in s.list::<Task>().unwrap() {
            t.active = true;
            s.put(&t).unwrap();
        }
    }

    // Details on things.
    let wh = item_id(&mut api, "water-heater");
    call(
        &mut api,
        "update_item",
        json!({"id": wh, "make": "Rheem", "model": "XG40T06EC36U1", "serial": "Q101902345",
               "installed": "2017-05-12", "purchased": "2017-05-01"}),
    );
    let furnace = item_id(&mut api, "furnace");
    call(
        &mut api,
        "update_item",
        json!({"id": furnace, "make": "Carrier", "model": "59SC5A060E17", "installed": "2019-10-03"}),
    );
    {
        let s = api.store_mut();
        let mut f: Item = s.fetch(&furnace).unwrap();
        f.warranty = Some(Warranty {
            expires: jiff::civil::date(2029, 10, 3),
            provider: Some("Carrier".into()),
            transferable: true,
            reference: Some(Flagged::sensitive("POL-99-1234-X".into())),
            months: Default::default(),
            register_url: Default::default(),
        });
        f.location_note = Some(Flagged::normal("Basement, by the stairs".into()));
        s.put(&f).unwrap();
        let mut w: Item = s.fetch(&wh).unwrap();
        w.warranty = Some(Warranty {
            expires: jiff::civil::date(2023, 5, 12),
            provider: Some("Rheem".into()),
            transferable: false,
            reference: None,
            months: Default::default(),
            register_url: Default::default(),
        });
        s.put(&w).unwrap();
        // A whole thing that is sensitive: a safe.
        let p: Property = s.list::<Property>().unwrap().remove(0);
        s.put(&Item {
            id: ItemId::new(),
            property: p.id,
            room: None,
            parent: None,
            name: "Heirloom silver".into(),
            category: Some("other".into()),
            make: Some("SentrySafe".into()),
            model: None,
            serial: Some(Flagged::sensitive("SAFE-SN-00912".into())),
            purchased: None,
            installed: None,
            warranty: None,
            location_note: Some(Flagged::sensitive(
                "Bedroom closet, under the loose floorboard".into(),
            )),
            template: None,
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Sensitivity::Sensitive,
            price: Default::default(),
            value: Default::default(),
        })
        .unwrap();
    }
    // A sensitive custom field on a normal thing.
    let panel = item_id(&mut api, "electrical-panel");
    call(
        &mut api,
        "add_field",
        json!({"item_id": panel, "label": "Gun safe combination", "type": "text"}),
    );
    call(
        &mut api,
        "update_item",
        json!({"id": panel, "values": {"gun_safe_combination": "Gun safe combination 31-07-22"}}),
    );

    // The facts sheet, normal and sensitive.
    let water = fact(
        &mut api,
        "shutoffs",
        "Water main shutoff",
        "Basement, front wall, behind the furnace",
        Some("water_main"),
    );
    fact(
        &mut api,
        "shutoffs",
        "Gas shutoff",
        "At the meter, outside on the north side",
        Some("gas_shutoff"),
    );
    let elec = fact(
        &mut api,
        "shutoffs",
        "Electrical panel",
        "Garage, left of the door to the kitchen",
        Some("electrical_panel"),
    );
    fact(
        &mut api,
        "shutoffs",
        "Sump pump breaker",
        "Breaker 14, labelled \"sump\"",
        None,
    );
    fact(
        &mut api,
        "sizes",
        "Furnace filter size",
        "16 x 25 x 1",
        Some("hvac_filter_size"),
    );
    fact(
        &mut api,
        "sizes",
        "Smoke alarm battery",
        "9V",
        Some("smoke_alarm_battery"),
    );
    fact(
        &mut api,
        "finishes",
        "Kitchen walls",
        "Pale Oak, eggshell",
        None,
    );
    // A phone number, not an account: the owner marks it normal.
    call(
        &mut api,
        "save_fact",
        json!({"group": "utilities", "label": "Gas company emergency line",
               "value": "555-0144", "sensitive": false}),
    );
    fact(
        &mut api,
        "access",
        "Wi-Fi network and password",
        SECRETS[0],
        Some("wifi"),
    );
    fact(&mut api, "access", "Alarm panel code", SECRETS[1], None);
    fact(
        &mut api,
        "utilities",
        "Electricity provider and account",
        SECRETS[2],
        Some("electric_provider"),
    );
    fact(&mut api, "utilities", "Gas account", SECRETS[3], None);
    fact(&mut api, "access", "Spare key", SECRETS[4], None);
    for (id, shade) in [(&water, 40u8), (&elec, 160)] {
        api.call_upload(
            "attach",
            json!({"about_kind": "fact", "about_id": id, "name": "shutoff.png"}),
            &photo(shade),
        )
        .unwrap();
    }
    api.call_upload(
        "attach",
        json!({"about_kind": "item", "about_id": wh, "name": "heater.png"}),
        &photo(90),
    )
    .unwrap();
    api.call_upload(
        "attach",
        json!({"about_kind": "item", "about_id": wh, "name": "Rheem manual.pdf", "kind": "manual"}),
        b"%PDF-1.4\n%%EOF\n",
    )
    .unwrap();

    // Quirks only you know.
    let pid = call(&mut api, "binder", Value::Null);
    let _ = pid;
    let property: String = api.store().list::<Property>().unwrap()[0].id.to_string();
    call(
        &mut api,
        "add_note",
        json!({"about_kind": "property", "about_id": property,
               "text": "The back door sticks in damp weather: lift the handle while you turn the key."}),
    );
    call(
        &mut api,
        "add_note",
        json!({"about_kind": "property", "about_id": property, "text": SECRETS[5], "sensitive": true}),
    );
    call(
        &mut api,
        "add_note",
        json!({"about_kind": "item", "about_id": furnace,
               "text": "It clicks twice before it lights. That is normal."}),
    );

    // Contractors: a private rating and private notes that must never print.
    call(
        &mut api,
        "save_contractor",
        json!({"name": "Ace Plumbing", "trades": ["plumbing"], "phone": "555-0101",
               "emergency": true, "notes": SECRETS[9], "notes_private": true}),
    );
    call(
        &mut api,
        "save_contractor",
        json!({"name": "Bright Spark Electric", "trades": ["electrical"], "phone": "555-0102",
               "emergency": true, "notes": "Ask for Dana."}),
    );
    call(
        &mut api,
        "save_contractor",
        json!({"name": "Cool Air Heating", "trades": ["heating", "cooling"], "phone": "555-0103",
               "email": "office@coolair.example"}),
    );
    // The household's private opinion (D-28), saved the way the app does.
    let ace = call(&mut api, "contractors", Value::Null)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut api,
        "save_contractor",
        json!({"id": ace, "name": "Ace Plumbing", "trades": ["plumbing"], "phone": "555-0101",
               "emergency": true, "notes": SECRETS[9], "notes_private": true,
               "private_rating": 2, "would_hire_again": "no",
               "private_notes": PRIVATE_NOTE}),
    );
    // Values for the insurance inventory.
    let furnace_id = item_id(&mut api, "furnace");
    call(
        &mut api,
        "update_item",
        json!({"id": furnace_id, "price": "4,200", "value": "2500.00"}),
    );
    let mut cool = None;
    {
        let s = api.store_mut();
        for mut c in s.list::<Contractor>().unwrap() {
            c.private_rating = Some(2);
            if c.name == "Cool Air Heating" {
                c.licence = Some(Credential {
                    number: Flagged::sensitive(SECRETS[10].into()),
                    expires: Some(jiff::civil::date(2027, 6, 30)),
                });
                cool = Some(c.id);
            }
            s.put(&c).unwrap();
        }
    }

    // History: some work done over the past two years.
    let tasks = api.store().list::<Task>().unwrap();
    for (i, t) in tasks.iter().enumerate().take(9) {
        let at = NOW_MS as i64 - (40 + 70 * i as i64) * DAY_MS;
        let done = call(
            &mut api,
            "complete",
            json!({"id": t.id.to_string(), "at_ms": at}),
        );
        if i % 2 == 0 {
            call(
                &mut api,
                "add_details",
                json!({"completion_id": done["completion_id"], "costs": {"parts": "24.50", "labour": "60"}}),
            );
        }
    }
    // One by the second member, one by a contractor.
    {
        let s = api.store_mut();
        let t = &tasks[9];
        s.append(&Completion {
            id: CompletionId::new(),
            task: t.id,
            by: member,
            at: jiff::Timestamp::from_millisecond(NOW_MS as i64 - 20 * DAY_MS).unwrap(),
            notes: None,
            cost: None,
            values: Default::default(),
            contractor: cool,
            attachments: vec![],
            voids: None,
            done_by: None,
        })
        .unwrap();
    }
    call(
        &mut api,
        "set_sealed_location",
        json!({"text": "In the fire safe, top shelf of the hall closet"}),
    );
    Demo { api, owner, member }
}

fn build(d: &Demo, e: Edition) -> binder::Binder {
    let data = binder::gather(d.api.store(), None, d.api.store().now()).unwrap();
    binder::build(&data, e, Options::for_edition(e))
}

fn images(api: &mut Api, doc: &binder::Document) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut m = std::collections::BTreeMap::new();
    for f in doc.photo_files() {
        let sha = &f[4..f.len() - 4];
        if let Some(b) = api.blobs().get(sha).unwrap() {
            m.insert(f, b);
        }
    }
    m
}

fn render(d: &mut Demo, doc: &binder::Document) -> binder::Rendered {
    let imgs = images(&mut d.api, doc);
    binder::pdf(doc, jiff::civil::date(2026, 11, 27), Paper::Letter, &imgs).unwrap()
}

/// Text of a PDF as a PDF reader sees it, when `pdftotext` is installed.
fn pdftotext(pdf: &[u8]) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("hw-binder-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let p = dir.join(format!("{}.pdf", housewright_core::blobs::sha256_hex(pdf)));
    std::fs::write(&p, pdf).ok()?;
    let out = std::process::Command::new("pdftotext")
        .arg(&p)
        .arg("-")
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&p);
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// Squash whitespace so text broken over lines still matches.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn save(name: &str, pdf: &[u8]) {
    if let Ok(dir) = std::env::var("HW_BINDER_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join(name), pdf).unwrap();
    }
}

#[test]
fn no_sensitive_value_reaches_a_main_volume() {
    let mut d = demo();
    for e in Edition::ALL {
        let b = build(&d, e);
        // Document model.
        let texts = b.main.texts().join("\n");
        for s in SECRETS {
            assert!(!texts.contains(s), "{e:?} main volume model holds {s:?}");
        }
        // The laid-out pages and the PDF as a reader extracts it.
        let r = render(&mut d, &b.main);
        let laid = squash(&r.page_text.join("\n"));
        let pdf = pdftotext(&r.pdf).map(|t| squash(&t));
        if pdf.is_none() {
            eprintln!("pdftotext not installed: checking laid-out text only");
        }
        for s in SECRETS {
            let s = squash(s);
            assert!(!laid.contains(&s), "{e:?} main PDF pages hold {s:?}");
            if let Some(p) = &pdf {
                assert!(!p.contains(&s), "{e:?} main PDF text holds {s:?}");
            }
        }
        save(&format!("{}-main.pdf", e.as_str()), &r.pdf);
        binder::check_main(
            &binder::gather(d.api.store(), None, d.api.store().now()).unwrap(),
            &b.main,
        )
        .unwrap();
    }
}

#[test]
fn the_sealed_volume_holds_the_sensitive_values() {
    let mut d = demo();
    for e in [Edition::Full, Edition::Succession] {
        let b = build(&d, e);
        let sealed = b.sealed.expect("has a sealed volume");
        let r = render(&mut d, &sealed);
        let laid = squash(&r.page_text.join("\n"));
        for s in SECRETS {
            assert!(laid.contains(&squash(s)), "{e:?} sealed volume lacks {s:?}");
        }
        assert!(laid.contains("SEALED"));
        assert!(laid.contains("Sync is off, so there is no recovery key yet"));
        // Not the ordinary things.
        assert!(!laid.contains("Pale Oak"));
        save(&format!("{}-sealed.pdf", e.as_str()), &r.pdf);
    }
    assert!(build(&d, Edition::Handover).sealed.is_none());
    assert!(build(&d, Edition::Emergency).sealed.is_none());
}

#[test]
fn the_main_volume_says_where_the_sealed_volume_is() {
    let mut d = demo();
    let b = build(&d, Edition::Full);
    let t = b.main.texts().join("\n");
    assert!(t.contains("In the fire safe, top shelf of the hall closet"));
    // Unset: a line to write on.
    call(&mut d.api, "set_sealed_location", json!({"text": ""}));
    let b = build(&d, Edition::Full);
    let about = b.main.sections.iter().find(|s| s.key == "about").unwrap();
    assert!(
        about
            .blocks
            .iter()
            .any(|x| matches!(x, binder::doc::Block::WriteIn { .. }))
    );
    // Succession: accounts point to the sealed volume.
    let b = build(&d, Edition::Succession);
    let acc = b
        .main
        .sections
        .iter()
        .find(|s| s.key == "accounts")
        .unwrap();
    let t = acc.texts().join("\n");
    assert!(
        t.contains("sealed volume, section \"Accounts to transfer\""),
        "{t}"
    );
    assert!(t.contains("Gas company emergency line"));
}

#[test]
fn the_private_rating_is_never_printed() {
    let mut d = demo();
    // The rating is 2 for every contractor; look for any way of writing it.
    for e in Edition::ALL {
        let b = build(&d, e);
        let mut docs = vec![b.main];
        docs.extend(b.sealed);
        for doc in docs {
            let t = doc.texts().join("\n").to_lowercase();
            assert!(!t.contains("rating"), "{e:?}");
            assert!(!t.contains("2/5") && !t.contains("2 of 5") && !t.contains("★"));
            let r = render(&mut d, &doc);
            assert!(!r.page_text.join(" ").to_lowercase().contains("rating"));
        }
    }
    // And the gathered data does not even carry it, or the other private
    // fields.
    let data = binder::gather(d.api.store(), None, d.api.store().now()).unwrap();
    assert!(data.contractors.iter().all(|c| c.private_rating.is_none()
        && c.would_hire_again.is_none()
        && c.private_notes.is_none()));
    for e in Edition::ALL {
        let b = build(&d, e);
        for doc in std::iter::once(b.main).chain(b.sealed) {
            assert!(!doc.texts().join("\n").contains("PRIVATE-NOTE"), "{e:?}");
        }
    }
    // The app itself still shows them to the household.
    let list = call(&mut d.api, "contractors", Value::Null);
    let ace = list
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Ace Plumbing")
        .unwrap();
    assert_eq!(ace["private"]["notes"], PRIVATE_NOTE);
    assert_eq!(ace["private"]["would_hire_again"], false);
}

#[test]
fn the_handover_edition_leaves_people_out() {
    let mut d = demo();
    let b = build(&d, Edition::Handover);
    let t = b.main.texts().join("\n");
    for name in ["Morgan", "Sam Rivera", "The Riveras", "Ask for Dana"] {
        assert!(!t.contains(name), "handover holds {name:?}");
    }
    assert!(t.contains("Previous owner"));
    // Costs are left out by default, and can be put back.
    assert!(!t.contains("$84.50"));
    let data = binder::gather(d.api.store(), None, d.api.store().now()).unwrap();
    let with = binder::build(
        &data,
        Edition::Handover,
        Options {
            include_costs: true,
            include_notes: true,
        },
    );
    assert!(with.main.texts().join("\n").contains("$84.50"));
    // A contractor who did the work is still named: that is about the house.
    assert!(t.contains("Cool Air Heating"));
    // Warranties say whether they transfer.
    assert!(t.contains("Transfers to a new owner"));
    assert!(t.contains("Does not transfer"));
    let r = render(&mut d, &b.main);
    assert!(!r.page_text.join(" ").contains("Morgan"));
    let _ = (&d.owner, d.member);
}

#[test]
fn the_emergency_page_fits_on_one_page() {
    let mut d = demo();
    let b = build(&d, Edition::Emergency);
    assert_eq!(b.main.sections.len(), 1);
    let r = render(&mut d, &b.main);
    assert_eq!(r.pages, 1, "the emergency page must be one page");
    let t = r.page_text.join(" ");
    for want in [
        "Water main shutoff",
        "Basement, front wall",
        "Gas smell",
        "Carbon monoxide",
        "Water leak",
        "No power",
        "Ace Plumbing",
        "555-0101",
        "Gas company emergency line",
        "911",
    ] {
        assert!(squash(&t).contains(want), "emergency page lacks {want:?}");
    }
    save("emergency-a4.pdf", &{
        let imgs = images(&mut d.api, &b.main);
        let r = binder::pdf(&b.main, jiff::civil::date(2026, 11, 27), Paper::A4, &imgs).unwrap();
        assert_eq!(r.pages, 1, "one page on A4 too");
        r.pdf
    });
    // It is also page 1 of the full binder.
    let full = build(&d, Edition::Full);
    assert_eq!(full.main.sections[0], b.main.sections[0]);
}

#[test]
fn every_page_carries_its_print_date_and_section_page_number() {
    let mut d = demo();
    let b = build(&d, Edition::Full);
    let r = render(&mut d, &b.main);
    assert!(r.pages > b.main.sections.len());
    for (i, p) in r.page_text.iter().enumerate() {
        assert!(p.contains("Printed Nov 27, 2026"), "page {i} has no date");
    }
    let all = r.page_text.join("\n");
    assert!(all.contains("Emergency 1"));
    assert!(all.contains("About 1"));
    // Numbered per section: some section runs past one page and the next
    // starts again at 1.
    assert!(
        all.contains("Heating 2") || all.contains("Water 2"),
        "{all}"
    );
    assert!(all.contains("History 1"));
}

#[test]
fn hostile_text_is_printed_as_text() {
    let mut d = demo();
    let hostile = [
        "#set page(width: 1cm)",
        "#import \"@preview/x:0.1.0\": *",
        "$x^2$ and *bold* and _it_",
        "`code` \\ backslash \" quote",
        "#read(\"/etc/passwd\")",
        "] #pagebreak() [",
        "<label> @ref // comment /* block",
    ];
    let property: String = d.api.store().list::<Property>().unwrap()[0].id.to_string();
    for h in hostile {
        call(
            &mut d.api,
            "add_note",
            json!({"about_kind": "property", "about_id": property, "text": h, "sensitive": false}),
        );
    }
    call(
        &mut d.api,
        "save_contractor",
        json!({"name": "#set text(size: 200pt)", "trades": ["$"], "phone": "\\u{0}"}),
    );
    let b = build(&d, Edition::Full);
    let house = b.main.only(&["house".into(), "contacts".into()]);
    let r = render(&mut d, &house);
    let laid = r.page_text.join("");
    for h in hostile {
        // Smart quotes aside, every character comes out as written.
        let want: String = h.chars().filter(|c| !"\"'".contains(*c)).collect();
        let got: String = laid.chars().filter(|c| !"\"'“”‘’".contains(*c)).collect();
        assert!(got.contains(&want), "{h:?} was not printed literally");
    }
    assert!(laid.contains("#set text(size: 200pt)"));
    // Nothing took effect: the pages are still letter-sized and few.
    assert!(r.pages <= 4, "{} pages", r.pages);
}

#[test]
fn a_section_fingerprint_changes_only_with_its_content() {
    let mut d = demo();
    let before = build(&d, Edition::Full).main;
    // Re-gathering a day later changes nothing printed... except what
    // depends on the date, so compare with the same clock.
    let again = build(&d, Edition::Full).main;
    for (a, b) in before.sections.iter().zip(&again.sections) {
        assert_eq!(a.fingerprint(), b.fingerprint(), "{}", a.key);
    }
    // Change a fact: only the facts section changes.
    fact(
        &mut d.api,
        "finishes",
        "Bathroom tiles",
        "White, 3 x 6",
        None,
    );
    let after = build(&d, Edition::Full).main;
    let changed: Vec<&str> = before
        .sections
        .iter()
        .zip(&after.sections)
        .filter(|(a, b)| a.fingerprint() != b.fingerprint())
        .map(|(a, _)| a.key.as_str())
        .collect();
    assert_eq!(changed, vec!["facts"]);
    // A sensitive change touches the sealed volume, never the main one
    // (beyond the count of facts kept there).
    let sealed_before = build(&d, Edition::Full).sealed.unwrap();
    let main_before = build(&d, Edition::Full).main;
    call(
        &mut d.api,
        "save_fact",
        json!({"group": "access", "label": "Garage door code", "value": "8080"}),
    );
    let b = build(&d, Edition::Full);
    let changed_sealed: Vec<&str> = sealed_before
        .sections
        .iter()
        .zip(&b.sealed.as_ref().unwrap().sections)
        .filter(|(x, y)| x.fingerprint() != y.fingerprint())
        .map(|(x, _)| x.key.as_str())
        .collect();
    assert_eq!(changed_sealed, vec!["sealed:codes"]);
    let changed_main: Vec<&str> = main_before
        .sections
        .iter()
        .zip(&b.main.sections)
        .filter(|(x, y)| x.fingerprint() != y.fingerprint())
        .map(|(x, _)| x.key.as_str())
        .collect();
    assert_eq!(
        changed_main,
        vec!["facts"],
        "only the 'kept in the sealed volume' count"
    );
}

#[test]
fn printing_records_what_changed_and_reprints_only_that() {
    let mut d = demo();
    let st = call(&mut d.api, "binder", Value::Null);
    let full = &st["editions"][1];
    assert_eq!(full["edition"], "full");
    assert!(full["last_printed"].is_null());
    let n = full["sections"].as_array().unwrap().len() as u64;
    assert_eq!(full["changed"], n, "never printed: everything is new");
    assert!(
        st["reminder"].is_null(),
        "no reminder before the first print"
    );

    let main = d
        .api
        .call_download("binder_pdf", json!({"edition": "full"}))
        .unwrap();
    assert!(main.starts_with(b"%PDF"));
    let sealed = d
        .api
        .call_download("binder_pdf", json!({"edition": "full", "volume": "sealed"}))
        .unwrap();
    assert!(sealed.starts_with(b"%PDF"));
    let st = call(&mut d.api, "binder", Value::Null);
    assert_eq!(st["editions"][1]["changed"], 0);
    assert_eq!(st["editions"][1]["last_printed"], "2026-11-27");
    // Other editions are tracked on their own.
    assert_ne!(st["editions"][0]["changed"], 0);
    // The yearly reminder now exists, switched off: offered, not imposed.
    assert_eq!(st["reminder"]["active"], false);

    // Nothing changed: nothing to reprint.
    assert!(
        d.api
            .call_download(
                "binder_pdf",
                json!({"edition": "full", "only_changed": true})
            )
            .is_err()
    );
    fact(&mut d.api, "sizes", "Dryer vent", "4 in", None);
    let st = call(&mut d.api, "binder", Value::Null);
    let changed: Vec<String> = st["editions"][1]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["changed"] == true)
        .map(|s| s["key"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(changed, vec!["facts".to_string()]);
    let partial = d
        .api
        .call_download(
            "binder_pdf",
            json!({"edition": "full", "only_changed": true}),
        )
        .unwrap();
    let text = pdftotext(&partial);
    if let Some(t) = text {
        assert!(t.contains("Dryer vent"));
        assert!(!t.contains("In an emergency"), "only the changed section");
        assert!(t.contains("Facts 1"));
    }
    save("full-reprint-facts.pdf", &partial);
    let st = call(&mut d.api, "binder", Value::Null);
    assert_eq!(st["editions"][1]["changed"], 0);

    // Switching the reminder on is the person's choice.
    let st = call(&mut d.api, "set_binder_reminder", json!({"on": true}));
    assert_eq!(st["reminder"]["active"], true);
}

#[test]
fn only_the_owner_prints_the_sealed_volume() {
    let mut d = demo();
    let m = d.member;
    d.api.store_mut().set_author(m).unwrap();
    let e = d
        .api
        .call_download("binder_pdf", json!({"edition": "full", "volume": "sealed"}))
        .unwrap_err();
    assert!(e.message.contains("owner"), "{}", e.message);
    assert!(
        d.api
            .call_download("binder_pdf", json!({"edition": "full", "record": false}))
            .is_ok()
    );
    let st = call(&mut d.api, "binder", Value::Null);
    assert_eq!(st["can_print_sealed"], false);
    // A member does not see a private note's text.
    let notes = call(&mut d.api, "contractors", Value::Null);
    let ace = notes
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Ace Plumbing")
        .unwrap();
    assert!(ace["notes"].is_null());
    assert_eq!(ace["notes_private"], true);
}

#[test]
fn quirks_notes_can_be_added_and_removed() {
    let mut d = demo();
    let furnace = item_id(&mut d.api, "furnace");
    let v = call(
        &mut d.api,
        "notes",
        json!({"about_kind": "item", "about_id": furnace}),
    );
    assert_eq!(v.as_array().unwrap().len(), 1);
    // A hiding place is sensitive by its words.
    let v = call(
        &mut d.api,
        "add_note",
        json!({"about_kind": "item", "about_id": furnace, "text": "The spare key is taped behind it"}),
    );
    assert_eq!(v[1]["sensitive"], true);
    let id = v[0]["id"].as_str().unwrap().to_string();
    call(&mut d.api, "delete_note", json!({"id": id}));
    let v = call(
        &mut d.api,
        "notes",
        json!({"about_kind": "item", "about_id": furnace}),
    );
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[test]
fn every_edition_typesets() {
    let mut d = demo();
    for e in Edition::ALL {
        for vol in ["main", "sealed"] {
            let r = d.api.call_download(
                "binder_pdf",
                json!({"edition": e.as_str(), "volume": vol, "record": false}),
            );
            if vol == "sealed" && !e.has_sealed() {
                assert!(r.is_err());
            } else {
                assert!(r.unwrap().starts_with(b"%PDF"), "{e:?} {vol}");
            }
        }
    }
}

#[test]
fn an_older_database_still_opens_and_prints() {
    // A Phase 4 export (schema 3, export version 3) imports and prints.
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/export-v2-phase3.json"
    ))
    .unwrap();
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut api = Api::new(store).unwrap();
    call(&mut api, "import", json!({ "json": json }));
    let pdf = api
        .call_download("binder_pdf", json!({"edition": "full"}))
        .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn the_emergency_page_stays_one_page_when_crowded() {
    let mut d = demo();
    let long = "Down the basement stairs, turn left past the old coal chute, behind the shelves of paint tins, low on the wall under the window, painted blue";
    for i in 0..6 {
        let id = fact(
            &mut d.api,
            "shutoffs",
            &format!("Extra shutoff number {i} with a long name"),
            long,
            None,
        );
        d.api
            .call_upload(
                "attach",
                json!({"about_kind": "fact", "about_id": id, "name": "s.png"}),
                &photo(30 * i as u8),
            )
            .unwrap();
    }
    for i in 0..6 {
        call(
            &mut d.api,
            "save_contractor",
            json!({"name": format!("Emergency Contractor Number {i} Limited"), "trades": ["plumbing", "heating", "roofing"],
                   "phone": "555-0199 extension 4412", "email": "dispatch@contractor.example", "emergency": true}),
        );
    }
    let b = build(&d, Edition::Emergency);
    let r = render(&mut d, &b.main);
    assert_eq!(
        r.pages, 1,
        "a crowded emergency page must still be one page"
    );
    let imgs = images(&mut d.api, &b.main);
    let a4 = binder::pdf(&b.main, jiff::civil::date(2026, 11, 27), Paper::A4, &imgs).unwrap();
    assert_eq!(a4.pages, 1, "and on A4");
    save("emergency-crowded.pdf", &r.pdf);
}

#[test]
fn the_web_path_bundle_render_record_matches_the_native_one() {
    // The web typesets in a separate module: bundle here, PDF there, then
    // record the receipt. Same result as binder_pdf.
    let mut d = demo();
    let zip = d
        .api
        .call_download("binder_bundle", json!({"edition": "full"}))
        .unwrap();
    let st = call(&mut d.api, "binder", Value::Null);
    assert_eq!(
        st["editions"][1]["last_printed"],
        Value::Null,
        "a bundle alone is not a print"
    );
    let r = binder::render::render_bundle(&zip).unwrap();
    assert!(r.pdf.starts_with(b"%PDF"));
    assert!(r.pages > 10);
    let receipt = binder::bundle::Bundle::from_zip(&zip).unwrap().receipt;
    let st = call(
        &mut d.api,
        "record_binder_print",
        serde_json::to_value(&receipt).unwrap(),
    );
    assert_eq!(st["editions"][1]["last_printed"], "2026-11-27");
    let main: Vec<&Value> = st["editions"][1]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["volume"] == "main")
        .collect();
    assert!(main.iter().all(|s| s["changed"] == false));
    // The bundle never carries a sensitive value either.
    let b = binder::bundle::Bundle::from_zip(&zip).unwrap();
    for s in SECRETS {
        assert!(!b.source.contains(s), "bundle source holds {s:?}");
    }
}

#[test]
fn handover_change_markers_follow_the_chosen_options() {
    // Found on the emulator: a print made with different choices than the
    // status compared with showed a section as changed straight away.
    let mut d = demo();
    d.api
        .call_download(
            "binder_pdf",
            json!({"edition": "handover", "include_costs": true}),
        )
        .unwrap();
    let same = call(&mut d.api, "binder", json!({"include_costs": true}));
    assert_eq!(same["editions"][2]["edition"], "handover");
    assert_eq!(same["editions"][2]["changed"], 0);
    let other = call(&mut d.api, "binder", json!({"include_costs": false}));
    let changed: Vec<&str> = other["editions"][2]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["changed"] == true)
        .map(|s| s["key"].as_str().unwrap())
        .collect();
    assert_eq!(changed, vec!["history"]);
}

// -- the insurance inventory (Phase 9) ---------------------------------------

fn insurance(d: &mut Demo, include_sensitive: bool) -> (Vec<u8>, String) {
    let pdf = d
        .api
        .call_download(
            "insurance_pdf",
            json!({"include_sensitive": include_sensitive}),
        )
        .unwrap();
    let csv = d
        .api
        .call_download(
            "insurance_csv",
            json!({"include_sensitive": include_sensitive}),
        )
        .unwrap();
    (pdf, String::from_utf8(csv).unwrap())
}

#[test]
fn the_insurance_inventory_leaves_sensitive_things_out_by_default() {
    let mut d = demo();
    let sum = call(&mut d.api, "insurance", Value::Null);
    assert!(sum["sensitive_things"].as_u64().unwrap() >= 1, "{sum}");
    assert_eq!(sum["can_include_sensitive"], true);
    let (pdf, csv) = insurance(&mut d, false);
    save("insurance.pdf", &pdf);
    // The document model and the laid-out pages.
    let data = binder::gather(d.api.store(), None, d.api.store().now()).unwrap();
    let opts = binder::insurance::InsuranceOptions::default();
    let inv = binder::insurance::inventory(&data, opts);
    let doc = binder::insurance::document(&data, &inv, opts);
    let texts = doc.texts().join("\n");
    let r = render(&mut d, &doc);
    let laid = r.page_text.join("\n");
    let reader = pdftotext(&pdf).map(|t| squash(&t));
    for secret in SECRETS {
        assert!(!texts.contains(secret), "document model: {secret}");
        assert!(!laid.contains(secret), "laid out: {secret}");
        assert!(!csv.contains(secret), "CSV: {secret}");
        if let Some(t) = &reader {
            assert!(!t.contains(&squash(secret)), "PDF text: {secret}");
        }
    }
    assert!(!texts.contains("PRIVATE-NOTE") && !csv.contains("PRIVATE-NOTE"));
    // Normal things are there, with their values, and the cover says what
    // was left out.
    assert!(texts.contains("Furnace"));
    assert!(texts.contains("$2,500.00") && texts.contains("$4,200.00"));
    assert!(texts.contains("Left out because they are marked sensitive"));
    assert!(csv.starts_with("\u{feff}Room,Thing,Make,Model,Serial"));
    assert!(csv.contains("4200.00,2500.00,USD"));
    assert!(csv.lines().last().unwrap().starts_with("Total,"));
    if let Some(t) = &reader {
        assert!(t.contains("Home inventory"));
    }
}

#[test]
fn the_insurance_inventory_includes_sensitive_things_when_asked_by_the_owner() {
    let mut d = demo();
    let (pdf, csv) = insurance(&mut d, true);
    save("insurance-sensitive.pdf", &pdf);
    assert!(csv.contains("SAFE-SN-00912"), "the safe's serial is there");
    assert!(
        csv.contains("POL-99-1234-X"),
        "the warranty reference is there"
    );
    if let Some(t) = pdftotext(&pdf).map(|t| squash(&t)) {
        assert!(t.contains("SAFE-SN-00912"));
        assert!(t.contains("Keep it somewhere safe"));
    }
    // Contractor details and private notes are never part of it.
    assert!(!csv.contains("PRIVATE-NOTE") && !csv.contains("HVAC-LIC-778812"));
    // A member may export, but not include sensitive items.
    d.api.store_mut().set_author(d.member).unwrap();
    assert!(
        d.api
            .call_download("insurance_csv", json!({"include_sensitive": true}))
            .is_err()
    );
    let csv = String::from_utf8(d.api.call_download("insurance_csv", json!({})).unwrap()).unwrap();
    assert!(!csv.contains("SAFE-SN-00912"));
}

#[test]
fn the_insurance_bundle_renders_like_the_native_pdf() {
    let mut d = demo();
    let zip = d.api.call_download("insurance_bundle", json!({})).unwrap();
    let r = binder::render::render_bundle(&zip).unwrap();
    assert!(r.pages >= 3);
    assert!(r.page_text.join(" ").contains("Totals by room"));
}
