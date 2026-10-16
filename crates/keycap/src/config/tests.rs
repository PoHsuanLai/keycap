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

/// What `capslock` writes in the default profile once `layer` is laid over the built-in mapping.
fn capslock_in(layer: ConfigLayer) -> Option<RuleAction> {
    let (defaults, _) = keycap_core::builtin();
    let (profiles, _) = resolve(&[defaults, layer]);
    rule(profiles.default_profile(), "capslock").cloned()
}

mod vendor_file {
    use std::path::Path;

    use peephole::FixtureProc;
    use peephole_core::{FileTrust, Uid, Writer};

    use super::super::vendor::{VENDOR_FILE_CAP, vendor, vendor_through};
    use super::*;

    const DIR: &str = "/usr/lib/keycap";
    const FILE: &str = "/usr/lib/keycap/keycap.toml";
    const CAPS_RULE: &str =
        "[profile.default]\nrules = [{ when = \"capslock\", then = \"ctrl+space\" }]\n";

    /// What the vendor layer in `fs` comes to: whether its rule applies, and the problems.
    fn read(fs: &FixtureProc) -> (Option<RuleAction>, Vec<String>) {
        let (layer, warnings) = vendor_through(fs, Path::new(DIR));
        let problems = warnings.into_iter().map(|w| w.problem).collect();
        (capslock_in(layer), problems)
    }

    #[test]
    fn a_vendor_file_is_a_layer_only_when_root_alone_can_change_it() {
        let rule = Some(map("ctrl+space"));
        let owner = Writer::Owner(Uid::new(1000));
        let cases: Vec<(&str, FixtureProc, Option<RuleAction>, &str)> = vec![
            (
                "root's own file",
                FixtureProc::new().file(FILE, CAPS_RULE, FileTrust::RootOnly),
                rule,
                "",
            ),
            (
                "a file its group can write",
                FixtureProc::new().file(FILE, CAPS_RULE, FileTrust::Writable(Writer::Group)),
                None,
                "not used",
            ),
            (
                "a file anyone can write",
                FixtureProc::new().file(FILE, CAPS_RULE, FileTrust::Writable(Writer::Other)),
                None,
                "not used",
            ),
            (
                "a file a person owns",
                FixtureProc::new().file(FILE, CAPS_RULE, FileTrust::Writable(owner)),
                None,
                "not used",
            ),
            (
                "a folder a person owns",
                FixtureProc::new()
                    .dir(DIR, FileTrust::Writable(owner))
                    .file(FILE, CAPS_RULE, FileTrust::RootOnly),
                None,
                "not used",
            ),
            (
                "a folder above that anyone can write",
                FixtureProc::new()
                    .dir("/usr/lib", FileTrust::Writable(Writer::Other))
                    .file(FILE, CAPS_RULE, FileTrust::RootOnly),
                None,
                "not used",
            ),
            (
                "a file that is not text",
                FixtureProc::new().file(FILE, vec![0xffu8, 0xfe], FileTrust::RootOnly),
                None,
                "not text",
            ),
            (
                "a file over the cap",
                FixtureProc::new().file(
                    FILE,
                    vec![b'#'; usize::try_from(VENDOR_FILE_CAP).unwrap() + 1],
                    FileTrust::RootOnly,
                ),
                None,
                "cannot be read",
            ),
            (
                "a file that is not a config",
                FixtureProc::new().file(FILE, "[profile\n", FileTrust::RootOnly),
                None,
                "installed defaults: ",
            ),
        ];
        for (case, fs, want, problem) in cases {
            let (got, problems) = read(&fs);
            assert_eq!(got, want, "{case}");
            match problem {
                "" => assert_eq!(problems, Vec::<String>::new(), "{case}"),
                text => {
                    assert_eq!(problems.len(), 1, "{case}: {problems:?}");
                    assert!(problems[0].contains(text), "{case}: {problems:?}");
                    assert!(problems[0].starts_with("installed defaults"), "{case}");
                }
            }
        }
    }

    #[test]
    fn a_missing_vendor_file_or_folder_is_no_layer_and_no_problem() {
        let trees = [
            ("no folder", FixtureProc::new()),
            (
                "an empty folder",
                FixtureProc::new().dir(DIR, FileTrust::RootOnly),
            ),
        ];
        for (case, fs) in trees {
            assert_eq!(read(&fs), (None, vec![]), "{case}");
        }
        let dir = Scratch::new("no-vendor");
        assert_eq!(
            vendor(&dir.0.join("not-there")),
            (ConfigLayer::default(), vec![])
        );
        assert_eq!(vendor(&dir.0), (ConfigLayer::default(), vec![]));
    }
}
