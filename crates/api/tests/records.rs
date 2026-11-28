//! Phase 4 records through the command surface: photos and documents,
//! custom fields, the facts sheet, costs, stickers and backups.

use std::io::Cursor;

use housewright_api::{Api, DOWNLOAD_COMMANDS, UPLOAD_COMMANDS};
use housewright_core::blobs::{BlobStore, MemoryBlobs, SqliteBlobs};
use housewright_core::store::{ManualClock, Store};
use serde_json::{Value, json};

/// 2026-11-27 12:00 UTC.
const NOW_MS: u64 = 1_795_780_800_000;

fn fresh() -> Api {
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    Api::with_blobs(store, Box::new(SqliteBlobs::in_memory().unwrap())).unwrap()
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn upload(api: &mut Api, cmd: &str, args: Value, bytes: &[u8]) -> Value {
    api.call_upload(cmd, args, bytes)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn setup() -> Api {
    let mut a = fresh();
    call(
        &mut a,
        "init",
        json!({"property_name": "Home", "your_name": "Alex", "time_zone": "America/New_York"}),
    );
    a
}

fn png(w: u32, h: u32, shade: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([shade, (x % 255) as u8, (y % 255) as u8])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn add(a: &mut Api, template: &str) -> String {
    call(a, "add_from_template", json!({"template": template}))["item_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn every_file_command_is_dispatched() {
    let mut a = setup();
    for cmd in UPLOAD_COMMANDS {
        if let Err(e) = a.call_upload(cmd, json!({}), b"x") {
            assert!(!e.message.contains("does not take"), "{cmd}");
        }
        assert!(a.call(cmd, json!({})).is_err(), "{cmd} needs the file call");
    }
    for cmd in DOWNLOAD_COMMANDS {
        if let Err(e) = a.call_download(cmd, json!({})) {
            assert!(!e.message.contains("does not return"), "{cmd}");
        }
    }
    fn send<T: Send>() {}
    send::<Api>();
}

#[test]
fn a_photo_on_a_thing_is_downscaled_with_a_thumbnail() {
    let mut a = setup();
    let item = add(&mut a, "water-heater");
    let att = upload(
        &mut a,
        "attach",
        json!({"about_kind": "item", "about_id": item, "name": "IMG_2041.png"}),
        &png(4000, 3000, 10),
    );
    assert_eq!(att["media_type"], "image/jpeg");
    assert_eq!(att["name"], "IMG_2041.jpg");
    assert_eq!(att["kind"], "photo");
    assert_eq!(
        (att["width"].as_u64(), att["height"].as_u64()),
        (Some(1600), Some(1200))
    );
    assert_eq!(att["has_thumb"], true);
    let id = att["id"].as_str().unwrap();
    let full = a
        .call_download("attachment_file", json!({"id": id}))
        .unwrap();
    let thumb = a
        .call_download("attachment_file", json!({"id": id, "thumb": true}))
        .unwrap();
    assert!(thumb.len() < full.len());
    let d = call(&mut a, "item", json!({"id": item}));
    assert_eq!(d["attachments"].as_array().unwrap().len(), 1);
    assert!(d["code"].as_str().unwrap().starts_with("HW-"));
    let h = call(&mut a, "history", Value::Null);
    let text = h[0]["text"].as_str().unwrap();
    assert!(text.ends_with("attached 'IMG_2041.jpg'"), "{text}");
}

#[test]
fn pdfs_are_kept_as_they_are_and_other_files_refused() {
    let mut a = setup();
    let item = add(&mut a, "furnace");
    let pdf = b"%PDF-1.4\n% a tiny test document\n%%EOF\n".to_vec();
    let att = upload(
        &mut a,
        "attach",
        json!({"about_kind": "item", "about_id": item, "name": "Manual.pdf"}),
        &pdf,
    );
    assert_eq!(att["media_type"], "application/pdf");
    assert_eq!(att["kind"], "manual");
    assert_eq!(att["has_thumb"], false);
    let back = a
        .call_download("attachment_file", json!({"id": att["id"]}))
        .unwrap();
    assert_eq!(back, pdf, "stored byte for byte");
    let e = a
        .call_upload(
            "attach",
            json!({"about_kind": "item", "about_id": item}),
            b"hello",
        )
        .unwrap_err();
    assert!(e.message.contains("PDF"));
    let e = a
        .call_upload(
            "attach",
            json!({"about_kind": "item", "about_id": "nope"}),
            &pdf,
        )
        .unwrap_err();
    assert_eq!(e.code, "not_found");
}

#[test]
fn same_file_twice_is_stored_once_and_deleting_frees_it() {
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut a = Api::with_blobs(store, Box::new(MemoryBlobs::default())).unwrap();
    call(
        &mut a,
        "init",
        json!({"property_name": "Home", "your_name": "Alex", "time_zone": "UTC"}),
    );
    let item = add(&mut a, "furnace");
    let pdf = b"%PDF-1.4\nsame\n".to_vec();
    let one = upload(
        &mut a,
        "attach",
        json!({"about_kind": "item", "about_id": item}),
        &pdf,
    );
    let two = upload(
        &mut a,
        "attach",
        json!({"about_kind": "item", "about_id": item}),
        &pdf,
    );
    assert_ne!(one["id"], two["id"]);
    // Deleting one leaves the bytes for the other.
    let r = call(&mut a, "delete_attachment", json!({"id": one["id"]}));
    assert_eq!(r["freed"], 0);
    assert!(
        a.call_download("attachment_file", json!({"id": two["id"]}))
            .is_ok()
    );
    let r = call(&mut a, "delete_attachment", json!({"id": two["id"]}));
    assert_eq!(r["freed"], 1);
    let list = call(
        &mut a,
        "attachments",
        json!({"about_kind": "item", "about_id": item}),
    );
    assert_eq!(list.as_array().unwrap().len(), 0);
    let h = call(&mut a, "history", Value::Null);
    assert!(h[0]["text"].as_str().unwrap().contains("deleted"));
}

#[test]
fn done_then_photo_cost_and_reading_are_optional_extras() {
    let mut a = setup();
    let item = add(&mut a, "sump-pump");
    let on = call(&mut a, "start_small", json!({"item_id": item, "n": 5}));
    let task = on
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["title"].as_str().unwrap().contains("Test"))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // P1: one tap, nothing asked first.
    let done = call(&mut a, "complete", json!({"id": task}));
    let cid = done["completion_id"].as_str().unwrap().to_string();
    let detail = call(&mut a, "task", json!({"id": task}));
    assert_eq!(detail["completion_fields"][0]["key"], "result");
    // Afterwards: a photo, costs and the pass/fail reading.
    upload(
        &mut a,
        "attach",
        json!({"about_kind": "completion", "about_id": cid}),
        &png(800, 600, 50),
    );
    let d = call(
        &mut a,
        "add_details",
        json!({"completion_id": cid, "note": "Float switch cleaned",
               "costs": {"parts": "12.50", "labour": "", "contractor": "$1,200"},
               "values": {"result": "pass"}}),
    );
    let c = &d["completions"][0];
    assert_eq!(c["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(c["cost_total"], 121250);
    assert_eq!(c["values"][0]["value"], "pass");
    assert_eq!(c["notes"][0], "Float switch cleaned");
    // A correction is a newer record, not an edit.
    let d = call(
        &mut a,
        "add_details",
        json!({"completion_id": cid, "costs": {"parts": "15"}}),
    );
    assert_eq!(d["completions"][0]["cost_total"], 1500);
    let e = a
        .call(
            "add_details",
            json!({"completion_id": cid, "costs": {"parts": "twelve"}}),
        )
        .unwrap_err();
    assert!(e.message.contains("amount"));

    let costs = call(&mut a, "costs", Value::Null);
    assert_eq!(costs["currency"], "USD");
    assert_eq!(costs["total"]["total"], 1500);
    assert_eq!(costs["total"]["parts"], 1500);
    assert_eq!(costs["by_year"][0]["key"], 2026);
    assert_eq!(costs["by_item"][0]["label"], "Sump pump");
    assert_eq!(costs["by_property"][0]["split"]["total"], 1500);
    // The audit says details were added, never the amounts.
    let h = call(&mut a, "history", Value::Null).to_string();
    assert!(h.contains("added details"));
    assert!(!h.contains("1500") && !h.contains("15.00"));

    // Undo the completion: its costs no longer count.
    call(&mut a, "void_completion", json!({"id": cid}));
    assert_eq!(call(&mut a, "costs", Value::Null)["total"]["total"], 0);
    let s = call(&mut a, "set_currency", json!({"currency": "cad"}));
    assert_eq!(s["currency"], "CAD");
    assert!(
        a.call("set_currency", json!({"currency": "dollars"}))
            .is_err()
    );
}

#[test]
fn fields_can_be_edited_added_and_hidden() {
    let mut a = setup();
    let item = add(&mut a, "boiler");
    let d = call(&mut a, "item", json!({"id": item}));
    let fuel = d["edit"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["def"]["key"] == "fuel")
        .unwrap()
        .clone();
    assert_eq!(fuel["origin"], "template");
    assert_eq!(fuel["has_value"], false);
    let before = d["tasks"].as_array().unwrap().len();

    // Fuel becomes known after onboarding: its service task is offered,
    // switched off, never switched on by itself.
    let out = call(
        &mut a,
        "update_item",
        json!({"id": item, "make": "Northwind", "values": {"fuel": "gas"}}),
    );
    let offered = out["offered"].as_array().unwrap();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0]["active"], false);
    assert_eq!(offered[0]["title"], "Book the yearly boiler service");
    assert_eq!(out["detail"]["tasks"].as_array().unwrap().len(), before + 1);
    assert_eq!(out["detail"]["make"], "Northwind");
    // Saving again offers nothing new.
    let again = call(
        &mut a,
        "update_item",
        json!({"id": item, "values": {"fuel": "gas"}}),
    );
    assert!(again["offered"].as_array().unwrap().is_empty());
    // Switch it on, then change the fuel: it is reported, not removed.
    call(
        &mut a,
        "set_active",
        json!({"id": offered[0]["id"], "active": true}),
    );
    let changed = call(
        &mut a,
        "update_item",
        json!({"id": item, "values": {"fuel": "oil"}}),
    );
    assert_eq!(changed["no_longer_apply"].as_array().unwrap().len(), 1);
    assert_eq!(changed["offered"].as_array().unwrap().len(), 1);

    // A field of the person's own: sensitive by default when it is a code.
    let d = call(
        &mut a,
        "add_field",
        json!({"item_id": item, "label": "Boiler room door code", "type": "text"}),
    );
    let code = d["edit"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["def"]["label"] == "Boiler room door code")
        .unwrap()
        .clone();
    assert_eq!(code["origin"], "item");
    assert_eq!(code["def"]["sensitivity"], "sensitive");
    let key = code["def"]["key"].as_str().unwrap().to_string();
    let out = call(
        &mut a,
        "update_item",
        json!({"id": item, "values": {key.clone(): "4096#"}}),
    );
    let text = out["detail"].to_string();
    assert!(!text.contains("4096#"), "sensitive values are not sent");
    let r = call(&mut a, "reveal_field", json!({"item_id": item, "key": key}));
    assert_eq!(r["value"], "4096#");
    assert!(
        !call(&mut a, "history", Value::Null)
            .to_string()
            .contains("4096#")
    );

    // A field for every boiler in this home.
    call(
        &mut a,
        "add_field",
        json!({"item_id": item, "label": "Expansion tank size", "type": "number", "unit": "gal", "to_template": true}),
    );
    let other = add(&mut a, "boiler");
    let d = call(&mut a, "item", json!({"id": other}));
    assert!(
        d["edit"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["def"]["key"] == "expansion_tank_size" && f["origin"] == "household")
    );
    // Adding the same thing again later (a library update) keeps it.
    let third = add(&mut a, "boiler");
    assert!(
        call(&mut a, "item", json!({"id": third}))["edit"]
            .to_string()
            .contains("expansion_tank_size")
    );
    // Bad values are refused; choice values must be options.
    assert!(
        a.call(
            "update_item",
            json!({"id": item, "values": {"fuel": "wood"}})
        )
        .is_err()
    );
    // A photo field holds one of the thing's photos.
    let d = call(
        &mut a,
        "add_field",
        json!({"item_id": item, "label": "Pressure gauge", "type": "photo"}),
    );
    assert!(d["edit"].to_string().contains("pressure_gauge"));
    let att = upload(
        &mut a,
        "attach",
        json!({"about_kind": "item", "about_id": item}),
        &png(300, 200, 9),
    );
    let out = call(
        &mut a,
        "update_item",
        json!({"id": item, "values": {"pressure_gauge": att["id"]}}),
    );
    let f = out["detail"]["edit"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["def"]["key"] == "pressure_gauge")
        .unwrap()
        .clone();
    assert_eq!(f["photo"]["id"], att["id"]);
    call(
        &mut a,
        "remove_field",
        json!({"item_id": item, "key": "pressure_gauge"}),
    );
}

#[test]
fn facts_sheet_hides_sensitive_values_everywhere() {
    let mut a = setup();
    let f = call(&mut a, "facts", Value::Null);
    assert_eq!(f["groups"][0]["label"], "Shutoffs and safety");
    assert_eq!(f["prompts"][0]["key"], "water_main");
    call(
        &mut a,
        "save_fact",
        json!({"group": "shutoffs", "label": "Water main shutoff", "value": "Basement, front wall", "key": "water_main"}),
    );
    let gate = call(
        &mut a,
        "save_fact",
        json!({"group": "access", "label": "Gate code", "value": "SECRET-7731"}),
    );
    assert_eq!(gate["sensitive"], true, "access is sensitive by default");
    assert!(gate["value"].is_null());
    let paint = call(
        &mut a,
        "save_fact",
        json!({"group": "finishes", "label": "Kitchen walls", "value": "Warm white, eggshell"}),
    );
    assert_eq!(paint["sensitive"], false);
    let f = call(&mut a, "facts", Value::Null);
    let text = f.to_string();
    assert!(text.contains("Basement, front wall"));
    assert!(!text.contains("SECRET-7731"));
    assert!(
        f["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["key"] != "water_main"),
        "answered questions are not asked again"
    );
    let r = call(&mut a, "reveal_fact", json!({"id": gate["id"]}));
    assert_eq!(r["value"], "SECRET-7731");
    // Edit the label without re-entering the hidden value.
    let g2 = call(
        &mut a,
        "save_fact",
        json!({"id": gate["id"], "group": "access", "label": "Side gate code"}),
    );
    assert_eq!(g2["sensitive"], true);
    assert_eq!(
        call(&mut a, "reveal_fact", json!({"id": gate["id"]}))["value"],
        "SECRET-7731"
    );

    // Never in History, never in a reminder.
    let item = add(&mut a, "smoke-alarm");
    call(&mut a, "start_small", json!({"item_id": item, "n": 3}));
    let hist = call(&mut a, "history", Value::Null).to_string();
    assert!(!hist.contains("SECRET-7731"));
    let plan = call(&mut a, "reminder_plan", json!({"horizon_days": 400})).to_string();
    assert!(!plan.contains("SECRET-7731"));
    let test = call(&mut a, "test_reminder", json!({})).to_string();
    assert!(!test.contains("SECRET-7731"));
    // Deleting is a tombstone.
    call(&mut a, "delete_fact", json!({"id": paint["id"]}));
    assert!(
        !call(&mut a, "facts", Value::Null)
            .to_string()
            .contains("Warm white")
    );
}

#[test]
fn stickers_codes_and_scanning() {
    let mut a = setup();
    let item = add(&mut a, "water-heater");
    add(&mut a, "furnace");
    let sheet = call(&mut a, "label_sheet", Value::Null);
    assert_eq!(sheet["stickers"].as_array().unwrap().len(), 2);
    assert!(sheet["presets"].as_array().unwrap().len() >= 2);
    let s = sheet["stickers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["item_id"] == item.as_str())
        .unwrap()
        .clone();
    assert_eq!(s["url"], format!("housewright://item/{item}"));
    assert!(s["qr_svg"].as_str().unwrap().starts_with("<svg"));
    let code = s["code"].as_str().unwrap();
    // Typing the code (any case, no dash) finds the thing.
    let typed = code[3..].to_lowercase();
    let found = call(&mut a, "find_label", json!({"text": typed}));
    assert_eq!(found[0]["id"], item.as_str());
    assert!(
        call(&mut a, "find_label", json!({"text": "HW-ZZZZZ"}))
            .as_array()
            .unwrap()
            .is_empty()
    );

    let pdf = a
        .call_download("label_sheet_pdf", json!({"preset": "letter-30"}))
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(
        a.call_download("label_sheet_pdf", json!({"preset": "nope"}))
            .is_err()
    );

    // A photo of the sticker finds the thing.
    let img = housewright_media::qr::render(s["url"].as_str().unwrap(), 6).unwrap();
    let mut jpg = Cursor::new(Vec::new());
    img.write_to(&mut jpg, image::ImageFormat::Jpeg).unwrap();
    let scan = upload(&mut a, "scan_label", json!({}), &jpg.into_inner());
    assert_eq!(scan["items"][0]["id"], item.as_str());
    let scan = upload(&mut a, "scan_label", json!({}), &png(200, 200, 255));
    assert!(scan["items"].as_array().unwrap().is_empty());
}

#[test]
fn label_reading_without_models_says_so() {
    let mut a = setup();
    let e = a
        .call_upload("read_label", json!({}), &png(100, 100, 0))
        .unwrap_err();
    assert_eq!(e.code, "ocr_unavailable");
}

#[test]
fn full_backup_round_trips_with_files() {
    let mut a = setup();
    let item = add(&mut a, "water-heater");
    let photo = upload(
        &mut a,
        "attach",
        json!({"about_kind": "item", "about_id": item}),
        &png(500, 400, 77),
    );
    call(
        &mut a,
        "save_fact",
        json!({"group": "access", "label": "Wi-Fi password", "value": "hunter2"}),
    );
    let original = a
        .call_download("attachment_file", json!({"id": photo["id"]}))
        .unwrap();
    let zip = a.call_download("full_backup", Value::Null).unwrap();
    assert!(zip.starts_with(b"PK"));

    let mut b = fresh();
    let s = upload(&mut b, "restore_backup", json!({}), &zip);
    assert_eq!(s["initialized"], true);
    let copy = b
        .call_download("attachment_file", json!({"id": photo["id"]}))
        .unwrap();
    assert_eq!(copy, original);
    assert_eq!(
        call(&mut b, "item", json!({"id": item}))["attachments"][0]["id"],
        photo["id"]
    );
    // A restore needs an empty database.
    assert!(b.call_upload("restore_backup", json!({}), &zip).is_err());
    // A damaged backup is refused before anything is written.
    let mut bad = zip.clone();
    let n = bad.len();
    bad[n / 2] ^= 0xFF;
    let mut c = fresh();
    assert!(c.call_upload("restore_backup", json!({}), &bad).is_err());
    assert_eq!(call(&mut c, "status", Value::Null)["initialized"], false);

    // The plain JSON export still works, through the same restore call too.
    let json_export = call(&mut a, "export", Value::Null)["json"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(json_export.contains("\"attachment\""));
    let mut d = fresh();
    upload(&mut d, "restore_backup", json!({}), json_export.as_bytes());
    let e = d
        .call_download("attachment_file", json!({"id": photo["id"]}))
        .unwrap_err();
    assert_eq!(
        e.code, "not_found",
        "metadata only: the bytes were not in it"
    );
}

#[test]
fn blobs_survive_reopening_a_sqlite_blob_file() {
    let dir = std::env::temp_dir().join(format!("hw-api-blobs-{NOW_MS}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("blobs.db");
    let h = {
        let mut b = SqliteBlobs::open(&path).unwrap();
        b.put(b"kept").unwrap()
    };
    let b = SqliteBlobs::open(&path).unwrap();
    assert_eq!(b.get(&h).unwrap().unwrap(), b"kept");
    std::fs::remove_dir_all(dir).unwrap();
}
