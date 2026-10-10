//! The real kernel path: the built daemon grabbing a fake keyboard on `/dev/uinput`, remapping
//! and writing "keycap keyboard", read back from `/dev/input`.
//!
//! These create input devices, so each is `#[ignore]`d and asserts `KEYCAP_DEVICE_TESTS=1`;
//! CI runs them as root with `-- --ignored --test-threads=1` (one keyboard name, one daemon at
//! a time). Only `key_table` runs in a plain `cargo test`.

mod conflict;
mod key_table;
mod keymap;
mod leds;
mod lifecycle;
mod mapping;
mod pause;
mod seat;
mod support;
mod watchdog;

/// Fails when a test file is added to this folder but not declared above (it would silently
/// never run).
#[test]
fn every_test_file_is_included() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/devices");
    let main = std::fs::read_to_string(dir.join("main.rs")).expect("main.rs");
    for entry in std::fs::read_dir(&dir).expect("tests/devices is readable") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|x| x == "rs") {
            let name = path
                .file_stem()
                .expect("stem")
                .to_string_lossy()
                .into_owned();
            assert!(
                name == "main" || main.contains(&format!("mod {name};")),
                "tests/devices/{name}.rs is not included in main.rs"
            );
        }
    }
}
