//! The harness names keys itself; this is the one place that checks it against keycap's table.
//! It touches no device and runs in a plain `cargo test`.

use keycap::Key;
use keycap_devtest::KEYS;

#[test]
fn every_harness_key_has_the_code_keycap_gives_its_name() {
    for &(name, code) in KEYS {
        let key = Key::named(name).unwrap_or_else(|| panic!("keycap has no key named {name:?}"));
        assert_eq!(key.code(), code, "{name}");
        assert_eq!(key.name(), name, "code {code}");
    }
}
