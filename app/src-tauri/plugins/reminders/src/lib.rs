//! Local reminders for Housewright (D-18, D-62).
//!
//! All the work happens in the Android half of this plugin (`android/`):
//! AlarmManager alarms post notifications with Done and Reschedule buttons, a
//! broadcast receiver records Done through the core over JNI without opening
//! the app, and a boot receiver reschedules everything after a restart or an
//! app update. The commands (`sync`, `test`, `pending`, and the standard
//! permission commands) go straight from JavaScript to Kotlin; this crate only
//! registers the Kotlin class. On desktop the plugin does nothing.

use tauri::{
    Runtime,
    plugin::{Builder, TauriPlugin},
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("reminders")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _api.register_android_plugin("app.housewright.reminders", "RemindersPlugin")?;
            Ok(())
        })
        .build()
}
