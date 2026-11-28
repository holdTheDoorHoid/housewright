const COMMANDS: &[&str] = &[
    "sync",
    "test",
    "pending",
    "view_file",
    "share_file",
    "check_permissions",
    "request_permissions",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
