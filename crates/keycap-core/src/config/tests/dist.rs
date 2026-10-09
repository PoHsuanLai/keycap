//! Every TOML file under `dist/` is a config keycap reads: each parses with no warnings, and
//! lays over the built-in mapping with none either.

use std::fs;
use std::path::PathBuf;

use super::*;

fn dist_tomls() -> Vec<PathBuf> {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist");
    let mut files: Vec<PathBuf> = fs::read_dir(&dist)
        .expect("dist/ is readable")
        .map(|entry| entry.expect("dist/ entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_dist_toml_parses_and_resolves_with_no_warnings() {
    let files = dist_tomls();
    assert!(!files.is_empty(), "dist/ ships at least the Plasma overlay");
    for path in files {
        let text = fs::read_to_string(&path).expect("a dist file is UTF-8 text");
        let (layer, warnings) = ConfigLayer::parse(&text, ConfigSource::System);
        assert_eq!(warnings, vec![], "{} parses cleanly", path.display());
        let (defaults, default_warnings) = builtin();
        assert_eq!(default_warnings, vec![]);
        let (_, resolve_warnings) = resolve(&[defaults, layer]);
        assert_eq!(
            resolve_warnings,
            vec![],
            "{} resolves over the defaults",
            path.display()
        );
    }
}
