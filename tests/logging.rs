//! Verifies that ratada emits `log` diagnostics for degraded conditions, wired
//! end-to-end through the `log` facade. One representative case (an invalid
//! color override) covers the mechanism; the I/O error paths are not sensibly
//! unit-testable without inducing real I/O failures, so they rely on review and
//! on the behaviour-preserving nature of adding a log call.

use std::sync::Mutex;

use log::{Level, Metadata, Record};
use ratada::theme::{ColorOverrides, Palette, ThemeRegistry};

/// Captured `(level, message)` records. A single test owns this binary, so no
/// cross-test interference.
static RECORDS: Mutex<Vec<(Level, String)>> = Mutex::new(Vec::new());

struct Collector;

impl log::Log for Collector {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        RECORDS
            .lock()
            .expect("records lock")
            .push((record.level(), record.args().to_string()));
    }

    fn flush(&self) {}
}

static COLLECTOR: Collector = Collector;

/// Installs the capture logger. `set_logger` is process-global and one-shot;
/// this test binary owns it, so a repeated call is harmless.
fn init_logger() {
    let _ = log::set_logger(&COLLECTOR);
    log::set_max_level(log::LevelFilter::Trace);
}

fn warnings() -> Vec<String> {
    RECORDS
        .lock()
        .expect("records lock")
        .iter()
        .filter(|(level, _)| *level == Level::Warn)
        .map(|(_, message)| message.clone())
        .collect()
}

#[test]
fn invalid_color_override_warns_and_valid_does_not() {
    init_logger();
    RECORDS.lock().expect("records lock").clear();

    let base = ThemeRegistry::builtin().resolve("default");

    // A valid override plus the empty (= "no override") defaults must be silent.
    let valid = ColorOverrides {
        accent: "#010203",
        ..ColorOverrides::default()
    };
    let _ = Palette::resolve(base, &valid);
    assert!(
        warnings().is_empty(),
        "valid/empty overrides must not warn, got: {:?}",
        warnings()
    );

    // A non-empty, unparseable override warns and keeps the theme color.
    let invalid = ColorOverrides {
        accent: "definitely-not-a-color",
        ..ColorOverrides::default()
    };
    let palette = Palette::resolve(base, &invalid);
    assert_eq!(
        palette.accent, base.accent,
        "an invalid override must keep the theme color"
    );
    let warns = warnings();
    assert!(
        warns.iter().any(|m| m.contains("invalid color override")),
        "expected an invalid-override warning, got: {warns:?}"
    );
}
