//! Drive the `hw` binary through a short household session.

use std::path::{Path, PathBuf};
use std::process::Command;

fn hw(db: &Path, args: &[&str]) -> String {
    let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    let out = Command::new(env!("CARGO_BIN_EXE_hw"))
        .args(args)
        .env("HW_DB", db)
        .env("HW_NOW", "2026-11-27")
        .env("HW_TEMPLATES", templates)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "hw {args:?} failed: {}{}",
        text,
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

#[test]
fn session() {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("h.db");

    hw(&db, &["init", "--household", "Testers", "--owner", "Alex"]);
    hw(
        &db,
        &[
            "property",
            "add",
            "--name",
            "Home",
            "--first-frost-doy",
            "300",
        ],
    );
    hw(&db, &["room", "add", "--name", "Hall"]);
    let out = hw(
        &db,
        &[
            "item",
            "add",
            "--name",
            "Hall alarm",
            "--room",
            "Hall",
            "--template",
            "smoke-alarm",
            "--set",
            "power=battery",
        ],
    );
    assert!(out.contains("5 suggested tasks"), "{out}");
    assert!(hw(&db, &["task", "list"]).contains("no tasks switched on"));
    let out = hw(&db, &["start", "--n", "2"]);
    assert_eq!(out.lines().count(), 2, "{out}");
    let out = hw(
        &db,
        &["task", "done", "Test the smoke alarm", "--note", "ok"],
    );
    assert!(out.contains("next due 2026-12-27"), "{out}");
    let out = hw(&db, &["audit"]);
    assert!(
        out.contains("Alex marked 'Test the smoke alarm' done on 2026-11-27"),
        "{out}"
    );

    let file = dir.join("export.json");
    hw(&db, &["export", file.to_str().unwrap()]);
    let db2 = dir.join("copy.db");
    let out = hw(&db2, &["import", file.to_str().unwrap()]);
    assert!(out.contains("history kept"), "{out}");
    assert_eq!(
        hw(&db, &["task", "list", "--all"]),
        hw(&db2, &["task", "list", "--all"])
    );
    std::fs::remove_dir_all(&dir).ok();
}
