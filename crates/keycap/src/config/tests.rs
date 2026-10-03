use std::path::PathBuf;

use keycap_core::{ConfigLayer, Hold, Profile, RuleAction, Stroke, Trigger, resolve};

use super::*;

fn map(s: &str) -> RuleAction {
    RuleAction::Map(
        Stroke::parse(s).expect("test strokes are valid"),
        Hold::Restore,
    )
}

fn rule<'a>(p: &'a Profile, when: &str) -> Option<&'a RuleAction> {
    p.rules
        .get(&Trigger::parse(when).expect("test triggers are valid"))
}

/// A scratch directory under the system temp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("keycap-config-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir is creatable");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_missing_system_file_is_an_empty_layer_without_warning() {
    let dir = Scratch::new("missing");
    assert_eq!(system(&dir.0), (ConfigLayer::default(), vec![]));
}

#[test]
fn the_system_file_is_read_from_its_directory() {
    let dir = Scratch::new("present");
    std::fs::write(
        dir.0.join(FILE_NAME),
        "[profile.default]\nrules = [{ when = \"command+left\", then = \"home\" }]\n",
    )
    .expect("scratch file is writable");
    let (layer, warnings) = system(&dir.0);
    assert_eq!(warnings, vec![]);
    let (profiles, _) = resolve(&[layer]);
    assert_eq!(
        rule(profiles.default_profile(), "command+left"),
        Some(&map("home"))
    );
}

#[test]
fn an_unreadable_system_file_is_an_empty_layer_and_one_warning() {
    let dir = Scratch::new("unreadable");
    std::fs::create_dir(dir.0.join(FILE_NAME)).expect("a directory can stand in for the file");
    let (layer, warnings) = system(&dir.0);
    assert_eq!(layer, ConfigLayer::default());
    assert_eq!(warnings.len(), 1);
}
