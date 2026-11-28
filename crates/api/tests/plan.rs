//! Floor plans through the command surface (Phase 8, D-25): floors, rooms
//! drawn on a grid, walls with doors and windows, pins that put things in
//! rooms, undo and redo, the plan in the house binder (with private pins
//! kept out of the main volume) and a single floor as a PDF.
//!
//! `HW_BINDER_OUT=dir cargo test -p housewright-api --test plan` also
//! writes the PDFs to `dir`, to look at.

use housewright_api::Api;
use housewright_binder::{self as binder, Edition, Options};
use housewright_core::blobs::SqliteBlobs;
use housewright_core::export;
use housewright_core::store::{ManualClock, Store};
use serde_json::{Value, json};

const NOW_MS: u64 = 1_795_780_800_000;

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn fails(api: &mut Api, cmd: &str, args: Value) -> String {
    match api.call(cmd, args) {
        Ok(v) => panic!("{cmd} should have failed, gave {v}"),
        Err(e) => e.message,
    }
}

fn api() -> Api {
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut api = Api::with_blobs(store, Box::new(SqliteBlobs::in_memory().unwrap())).unwrap();
    call(
        &mut api,
        "init",
        json!({"property_name": "14 Larch Lane", "your_name": "Morgan", "time_zone": "America/New_York"}),
    );
    api
}

fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Value {
    json!([{"x": x0, "y": y0}, {"x": x1, "y": y0}, {"x": x1, "y": y1}, {"x": x0, "y": y1}])
}

fn p(x: i64, y: i64) -> Value {
    json!({"x": x, "y": y})
}

fn add_thing(api: &mut Api, template: &str, name: &str) -> String {
    call(
        api,
        "add_from_template",
        json!({"template": template, "name": name}),
    )["item_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn save(name: &str, pdf: &[u8]) {
    if let Ok(dir) = std::env::var("HW_BINDER_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join(name), pdf).unwrap();
    }
}

fn pdftotext(pdf: &[u8]) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("hw-plan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let f = dir.join(format!("{}.pdf", housewright_core::blobs::sha256_hex(pdf)));
    std::fs::write(&f, pdf).ok()?;
    let out = std::process::Command::new("pdftotext")
        .arg(&f)
        .arg("-")
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&f);
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Ground floor with a kitchen and an L-shaped living room drawn as
/// shapes, and a basement with walls, a door and a window.
struct House {
    api: Api,
    ground: String,
    basement: String,
    kitchen: String,
    wall: String,
}

fn house() -> House {
    let mut api = api();
    // No plan at all: everything else still works (P3).
    assert_eq!(call(&mut api, "floors", Value::Null), json!([]));
    let floors = call(&mut api, "save_floor", json!({"name": "Ground floor"}));
    let ground = floors[0]["id"].as_str().unwrap().to_string();
    let floors = call(
        &mut api,
        "save_floor",
        json!({"name": "Basement", "level": -1}),
    );
    assert_eq!(floors[0]["name"], "Basement", "lowest floor first");
    let basement = floors[0]["id"].as_str().unwrap().to_string();

    let k = call(
        &mut api,
        "save_room_shape",
        json!({"floor_id": ground, "name": "Kitchen", "outline": rect(0, 0, 3658, 3048)}),
    );
    let kitchen = k["ids"][0].as_str().unwrap().to_string();
    let l = json!([
        p(3658, 0),
        p(9754, 0),
        p(9754, 3048),
        p(7315, 3048),
        p(7315, 6096),
        p(3658, 6096)
    ]);
    call(
        &mut api,
        "save_room_shape",
        json!({"floor_id": ground, "name": "Living room", "outline": l}),
    );

    // Basement walls drawn point to point: a closed 6 x 4 m box.
    let w = call(
        &mut api,
        "save_walls",
        json!({"floor_id": basement, "walls": [
            {"a": p(0, 0), "b": p(6000, 0)},
            {"a": p(6000, 0), "b": p(6000, 4000)},
            {"a": p(6000, 4000), "b": p(0, 4000)},
            {"a": p(0, 4000), "b": p(0, 0)},
        ]}),
    );
    let wall = w["ids"][0].as_str().unwrap().to_string();
    House {
        api,
        ground,
        basement,
        kitchen,
        wall,
    }
}

#[test]
fn rooms_drawn_on_a_grid_have_areas_and_a_total() {
    let mut h = house();
    let plan = call(&mut h.api, "floor_plan", json!({"floor_id": h.ground}));
    let rooms = plan["rooms"].as_array().unwrap();
    assert_eq!(rooms.len(), 2);
    let kitchen = rooms.iter().find(|r| r["name"] == "Kitchen").unwrap();
    // 12' x 10' = 120 sq ft.
    assert_eq!(kitchen["area_text"], "120 sq ft");
    assert_eq!(kitchen["perimeter_text"], "44' 0\"");
    let living = rooms.iter().find(|r| r["name"] == "Living room").unwrap();
    assert_eq!(living["area_text"], "320 sq ft");
    assert_eq!(plan["total_area_text"], "440 sq ft");
    assert_eq!(plan["units"], "imperial");

    // Metres when the household prefers them.
    let st = call(&mut h.api, "set_units", json!({"units": "metric"}));
    assert_eq!(st["units"], "metric");
    let plan = call(&mut h.api, "floor_plan", json!({"floor_id": h.ground}));
    assert_eq!(plan["total_area_text"], "40.9 m²");

    // A crossed outline is refused, calmly.
    let msg = fails(
        &mut h.api,
        "save_room_shape",
        json!({"floor_id": h.ground, "name": "Bad", "outline": [p(0, 0), p(10, 10), p(10, 0), p(0, 10)]}),
    );
    assert_eq!(msg, "the outline crosses itself");

    // The rooms are ordinary rooms: they appear in the room list.
    let names: Vec<String> = call(&mut h.api, "rooms", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["Kitchen", "Living room"]);

    // Drawing a room named like an existing room without a plan reuses it.
    let den = call(&mut h.api, "add_room", json!({"name": "Den"}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(
        &mut h.api,
        "save_room_shape",
        json!({"floor_id": h.ground, "name": "den", "outline": rect(0, 4000, 3000, 7000)}),
    );
    assert_eq!(r["ids"][0], den);
}

#[test]
fn walls_make_spaces_and_take_doors_and_windows() {
    let mut h = house();
    let plan = call(&mut h.api, "floor_plan", json!({"floor_id": h.basement}));
    assert_eq!(plan["walls"].as_array().unwrap().len(), 4);
    assert_eq!(plan["walls"][0]["length_text"], "19' 8\"");
    let spaces = plan["spaces"].as_array().unwrap();
    assert_eq!(spaces.len(), 1, "the closed walls make one unnamed space");
    assert_eq!(spaces[0]["area_text"], "258 sq ft");

    // Naming the space makes it a room; it is no longer offered.
    let outline = spaces[0]["outline"].clone();
    let r = call(
        &mut h.api,
        "save_room_shape",
        json!({"floor_id": h.basement, "name": "Workshop", "outline": outline}),
    );
    assert!(r["plan"]["spaces"].as_array().unwrap().is_empty());

    let door = call(
        &mut h.api,
        "save_opening",
        json!({"wall_id": h.wall, "kind": "door", "offset": 1000, "width": 914,
               "swing": {"hinge": "a", "side": "right"}}),
    );
    assert_eq!(door["plan"]["openings"][0]["width_text"], "3' 0\"");
    let msg = fails(
        &mut h.api,
        "save_opening",
        json!({"wall_id": h.wall, "kind": "window", "offset": 1500, "width": 900}),
    );
    assert_eq!(msg, "that overlaps the door already on this wall");
    let msg = fails(
        &mut h.api,
        "save_opening",
        json!({"wall_id": h.wall, "kind": "window", "offset": 5500, "width": 900}),
    );
    assert!(
        msg.starts_with("that does not fit: the wall is 19' 8\" long"),
        "{msg}"
    );
    call(
        &mut h.api,
        "save_opening",
        json!({"wall_id": h.wall, "kind": "window", "offset": 3500, "width": 1200}),
    );
    let plan = call(&mut h.api, "delete_wall", json!({"id": h.wall}));
    assert!(
        plan["openings"].as_array().unwrap().is_empty(),
        "its door and window went with it"
    );
}

#[test]
fn pins_put_things_in_rooms_and_show_on_the_thing() {
    let mut h = house();
    let fridge = add_thing(&mut h.api, "refrigerator", "Fridge");
    let out = call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(1000, 1000), "item_id": fridge}),
    );
    assert_eq!(out["moved_to"], "Kitchen");
    let pin = out["ids"][0].as_str().unwrap().to_string();
    let detail = call(&mut h.api, "item", json!({"id": fridge}));
    assert_eq!(detail["row"]["room_name"], "Kitchen");
    assert_eq!(detail["pin"]["floor_name"], "Ground floor");
    assert_eq!(detail["pin"]["pin_id"], pin);

    // Placing the same thing again moves its one pin.
    let out = call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(5000, 1000), "item_id": fridge}),
    );
    assert_eq!(out["ids"][0], pin);
    assert_eq!(out["moved_to"], "Living room");
    assert_eq!(out["plan"]["pins"].as_array().unwrap().len(), 1);

    // A labelled pin needs no thing; facts can point at it.
    let out = call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(200, 2900), "label": "Water main"}),
    );
    let main = out["ids"][0].as_str().unwrap().to_string();
    let fact = call(
        &mut h.api,
        "save_fact",
        json!({"group": "shutoffs", "label": "Water main shutoff", "value": "Kitchen, under the sink", "key": "water_main", "pin_id": main}),
    );
    assert_eq!(fact["pin_id"], main);
    // Editing the fact without mentioning the pin keeps the link.
    let fact = call(
        &mut h.api,
        "save_fact",
        json!({"id": fact["id"], "group": "shutoffs", "label": "Water main shutoff", "value": "Kitchen sink, left"}),
    );
    assert_eq!(fact["pin_id"], main);
    let plan = call(&mut h.api, "floor_plan", json!({"floor_id": h.ground}));
    let pin = plan["pins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == main)
        .unwrap();
    assert_eq!(pin["fact_label"], "Water main shutoff");
    assert_eq!(pin["room_name"], "Kitchen");

    let msg = fails(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(0, 0)}),
    );
    assert_eq!(msg, "a pin needs a thing or a name");
    // Deleting the pin unlinks the fact.
    call(&mut h.api, "delete_pin", json!({"id": main}));
    let facts = call(&mut h.api, "facts", Value::Null);
    let s = facts.to_string();
    assert!(!s.contains(&main));
}

#[test]
fn undo_and_redo_restore_one_step() {
    let mut h = house();
    let before = call(&mut h.api, "floor_plan", json!({"floor_id": h.basement}))["state"].clone();
    call(
        &mut h.api,
        "save_opening",
        json!({"wall_id": h.wall, "kind": "door", "offset": 1000, "width": 900}),
    );
    let after_door =
        call(&mut h.api, "floor_plan", json!({"floor_id": h.basement}))["state"].clone();
    let after_delete = call(&mut h.api, "delete_wall", json!({"id": h.wall}))["state"].clone();
    assert_eq!(after_delete["walls"].as_array().unwrap().len(), 3);

    // Undo the delete: the wall and its door come back (under new ids).
    let r = call(
        &mut h.api,
        "plan_restore",
        json!({"floor_id": h.basement, "to": after_door, "from": after_delete}),
    );
    assert_eq!(r["plan"]["walls"].as_array().unwrap().len(), 4);
    assert_eq!(r["plan"]["openings"].as_array().unwrap().len(), 1);
    let new_wall = r["renamed"][&h.wall].as_str().unwrap().to_string();
    assert_eq!(r["plan"]["openings"][0]["wall_id"], new_wall);

    // Undo the door.
    let now = r["plan"]["state"].clone();
    let mut target = before.clone();
    // The interface renames ids in its history the same way.
    let t = target.to_string().replace(&h.wall, &new_wall);
    target = serde_json::from_str(&t).unwrap();
    let r = call(
        &mut h.api,
        "plan_restore",
        json!({"floor_id": h.basement, "to": target, "from": now}),
    );
    assert!(r["plan"]["openings"].as_array().unwrap().is_empty());
    assert_eq!(r["plan"]["walls"].as_array().unwrap().len(), 4);

    // An edit made meanwhile elsewhere on the floor is kept by an undo.
    let s1 = r["plan"]["state"].clone();
    let moved = call(
        &mut h.api,
        "save_walls",
        json!({"floor_id": h.basement, "walls": [{"id": new_wall, "a": p(0, 0), "b": p(6500, 0)}]}),
    )["plan"]["state"]
        .clone();
    call(
        &mut h.api,
        "save_label",
        json!({"floor_id": h.basement, "at": p(3000, 2000), "text": "Stairs up"}),
    );
    let r = call(
        &mut h.api,
        "plan_restore",
        json!({"floor_id": h.basement, "to": s1, "from": moved}),
    );
    assert_eq!(
        r["plan"]["labels"].as_array().unwrap().len(),
        1,
        "the label stays"
    );
    let w = r["plan"]["walls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == new_wall)
        .unwrap()
        .clone();
    assert_eq!(w["b"], p(6000, 0));
}

#[test]
fn limited_members_cannot_change_the_plan() {
    use housewright_core::ids::MemberId;
    use housewright_core::model::{Member, Role};
    let mut h = house();
    let m = MemberId::new();
    let s = h.api.store_mut();
    s.put(&Member {
        id: m,
        name: "Kid".into(),
        role: Role::Limited,
        kind: Default::default(),
        contact: None,
        reminders: None,
    })
    .unwrap();
    s.set_author(m).unwrap();
    let msg = fails(&mut h.api, "save_floor", json!({"name": "Attic"}));
    assert!(msg.starts_with("only the household's owner and members"));
    assert_eq!(
        call(&mut h.api, "floor_plan", json!({"floor_id": h.ground}))["can_edit"],
        false
    );
}

#[test]
fn plans_survive_export_and_import() {
    let mut h = house();
    let fridge = add_thing(&mut h.api, "refrigerator", "Fridge");
    call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(1000, 1000), "item_id": fridge}),
    );
    let e = export::export(h.api.store()).unwrap();
    assert_eq!(e.version, 6);
    let text = export::to_json(&e).unwrap();
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut b = Api::new(store).unwrap();
    call(&mut b, "import", json!({"json": text}));
    let a_plan = call(&mut h.api, "floor_plan", json!({"floor_id": h.ground}));
    let b_plan = call(&mut b, "floor_plan", json!({"floor_id": h.ground}));
    assert_eq!(a_plan, b_plan);
}

/// Pins and rooms for the binder: a water main linked to a fact, a
/// thing, a private pin and a private thing's pin.
fn binder_house() -> House {
    let mut h = house();
    let heater = add_thing(&mut h.api, "water-heater", "Water heater");
    call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.basement, "at": p(5500, 500), "item_id": heater}),
    );
    let main = call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.basement, "at": p(300, 3700), "label": "Water main"}),
    )["ids"][0]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &mut h.api,
        "save_fact",
        json!({"group": "shutoffs", "label": "Water main shutoff", "value": "Basement, front wall", "key": "water_main", "pin_id": main}),
    );
    call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(3000, 2500), "label": "Spare key under the blue flowerpot", "sensitive": true}),
    );
    call(
        &mut h.api,
        "save_label",
        json!({"floor_id": h.ground, "at": p(6000, 4500), "text": "Stairs down"}),
    );
    // A safe: a sensitive thing. Its pin must not be in the main volume.
    let safe = add_thing(&mut h.api, "refrigerator", "Wall safe");
    {
        use housewright_core::model::{Item, Sensitivity};
        let s = h.api.store_mut();
        let mut it: Item = s.fetch(&safe).unwrap();
        it.sensitivity = Sensitivity::Sensitive;
        s.put(&it).unwrap();
    }
    call(
        &mut h.api,
        "save_pin",
        json!({"floor_id": h.ground, "at": p(8000, 1000), "item_id": safe}),
    );
    call(
        &mut h.api,
        "save_opening",
        json!({"wall_id": h.wall, "kind": "door", "offset": 1000, "width": 914, "swing": {"hinge": "b", "side": "left"}}),
    );
    let walls = call(&mut h.api, "floor_plan", json!({"floor_id": h.basement}))["walls"].clone();
    let side = walls[1]["id"].as_str().unwrap().to_string();
    call(
        &mut h.api,
        "save_opening",
        json!({"wall_id": side, "kind": "window", "offset": 1200, "width": 1500}),
    );
    h
}

#[test]
fn the_binder_prints_one_page_per_floor_without_private_pins() {
    let h = binder_house();
    let data = binder::gather(h.api.store(), None, h.api.store().now()).unwrap();
    let b = binder::build(&data, Edition::Full, Options::default());
    let keys: Vec<&str> = b.main.sections.iter().map(|s| s.key.as_str()).collect();
    let plans: Vec<&&str> = keys.iter().filter(|k| k.starts_with("plan:")).collect();
    assert_eq!(plans.len(), 2, "{keys:?}");
    let text = b.main.texts().join(" | ");
    for want in [
        "Floor plan: Ground floor",
        "Floor plan: Basement",
        "Kitchen",
        "120 sq ft",
        "Water heater",
        "Stairs down",
        "Total",
    ] {
        assert!(text.contains(want), "main volume lacks {want:?}");
    }
    for secret in ["Spare key under the blue flowerpot", "Wall safe"] {
        assert!(!text.contains(secret), "{secret:?} reached the main volume");
    }
    assert!(
        text.contains("2 marked private pins or labels are not shown on these plans."),
        "{text}"
    );
    // The emergency page says where the water main is on the plan.
    assert!(text.contains("Basement plan, pin "), "{text}");
    binder::check_main(&data, &b.main).unwrap();

    let imgs = Default::default();
    let r = binder::pdf(
        &b.main,
        jiff::civil::date(2026, 11, 28),
        binder::Paper::Letter,
        &imgs,
    )
    .unwrap();
    save("plan-full-main.pdf", &r.pdf);
    let t = r.page_text.join(" ");
    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(t.contains("Floor plan: Basement"));
    assert!(!t.contains("flowerpot"));
    if let Some(pt) = pdftotext(&r.pdf) {
        assert!(!pt.contains("flowerpot"));
        assert!(!pt.contains("Wall safe"));
        assert!(pt.contains("Workshop") || pt.contains("Kitchen"));
    }
}

#[test]
fn one_floor_prints_on_its_own() {
    let mut h = binder_house();
    let data = binder::gather(h.api.store(), None, h.api.store().now()).unwrap();
    let key = format!("plan:{}", h.ground);
    let pdf = h
        .api
        .call_download(
            "binder_pdf",
            json!({"edition": "full", "volume": "main", "sections": [key], "record": false}),
        )
        .unwrap();
    save("plan-ground-floor.pdf", &pdf);
    assert!(pdf.starts_with(b"%PDF"));
    if let Some(t) = pdftotext(&pdf) {
        assert!(t.contains("Floor plan: Ground floor"), "{t}");
        assert!(!t.contains("Floor plan: Basement"));
        assert!(!t.contains("In an emergency"));
        assert!(t.contains("Kitchen"));
        assert!(
            t.contains("Ground floor 1") || t.contains("Plan Ground floor 1"),
            "{t}"
        );
    }
    // Printing one floor this way is not recorded as a binder print.
    let st = call(&mut h.api, "binder", Value::Null);
    assert!(
        st["editions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["last_printed"].is_null())
    );
    let _ = data;
    let _ = h.kitchen;
}
