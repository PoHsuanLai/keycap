//! `keycap replay FILE`: a script through the remapper with the built-in profiles, printing each
//! input and what it writes.

use super::script::{Step, parse};
use crate::daemon::settings::{Layers, parse_user};
use crate::{CoreStatus, Edge, OutEvent, Profiles, RemapOut, Remapper};
use std::path::Path;

/// The writes and requests of one step as text: `+key` and `-key` for presses and releases,
/// `~key` for a repeat, `SYN` for the sync, separated by spaces within one write and `; `
/// between outputs.
pub(super) fn render(outs: &[RemapOut]) -> String {
    let one = |out: &RemapOut| match out {
        RemapOut::Write(events) => events
            .iter()
            .map(render_event)
            .collect::<Vec<_>>()
            .join(" "),
        RemapOut::ReleaseGrabs => "release-grabs".to_owned(),
        RemapOut::Status(CoreStatus::Running) => "status running".to_owned(),
        RemapOut::Status(CoreStatus::Paused) => "status paused".to_owned(),
    };
    outs.iter().map(one).collect::<Vec<_>>().join("; ")
}

fn render_event(event: &OutEvent) -> String {
    match event {
        OutEvent::Key {
            key,
            edge: Edge::Press,
        } => format!("+{}", key.0.name()),
        OutEvent::Key {
            key,
            edge: Edge::Release,
        } => format!("-{}", key.0.name()),
        OutEvent::Key {
            key,
            edge: Edge::Repeat,
        } => format!("~{}", key.0.name()),
        OutEvent::Sync => "SYN".to_owned(),
    }
}

/// One line per step: the input as written, then `->` and what it wrote when it wrote anything.
pub(super) fn run(steps: Vec<Step>, profiles: &Profiles) -> Vec<String> {
    let mut remapper = Remapper::new();
    steps
        .into_iter()
        .map(|step| {
            let (next, outs) = remapper.clone().step(step.input, profiles);
            remapper = next;
            match outs.as_slice() {
                [] => step.text,
                outs => format!("{} -> {}", step.text, render(outs)),
            }
        })
        .collect()
}

/// Replays the script in `file` under the built-in profiles, with `config` laid over them as a
/// user layer when given. Config problems go to stderr; the printed lines are returned.
pub(super) fn replay_file(file: &Path, config: Option<&Path>) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let steps = parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let layers = match config {
        None => Layers::builtin_only(),
        Some(path) => {
            let toml =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let (layer, warnings) = parse_user(&toml);
            warnings
                .iter()
                .for_each(|w| eprintln!("keycap: config: {}: {}", w.key, w.problem));
            Layers::builtin_only().with_user(layer)
        }
    };
    let (profiles, warnings) = layers.resolve();
    warnings
        .iter()
        .for_each(|w| eprintln!("keycap: config: {}: {}", w.key, w.problem));
    Ok(run(steps, &profiles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::scratch::Scratch;

    const COPY: &str = "\
# Cmd+C in a GUI app, then in a terminal
0 press 1 apple leftmeta
10 press 1 apple c
20 release 1 apple c
30 release 1 apple leftmeta
40 focus terminal
50 press 1 apple leftmeta
60 press 1 apple c
70 release 1 apple c
80 release 1 apple leftmeta
";

    #[test]
    fn a_script_replays_to_exactly_what_the_core_writes() {
        let dir = Scratch::new("replay");
        let file = dir.path().join("copy.txt");
        std::fs::write(&file, COPY).unwrap();
        let lines = replay_file(&file, None).unwrap();
        assert_eq!(
            lines,
            [
                "0 press 1 apple leftmeta -> +leftctrl SYN",
                "10 press 1 apple c -> +c SYN",
                "20 release 1 apple c -> -c SYN",
                "30 release 1 apple leftmeta -> -leftctrl SYN",
                "40 focus terminal",
                "50 press 1 apple leftmeta -> +leftctrl SYN",
                "60 press 1 apple c -> +leftshift +c SYN",
                "70 release 1 apple c -> -c -leftshift SYN",
                "80 release 1 apple leftmeta -> -leftctrl SYN",
            ]
        );
    }

    #[test]
    fn a_user_config_changes_what_replays() {
        let dir = Scratch::new("replay-config");
        let file = dir.path().join("copy.txt");
        let config = dir.path().join("user.toml");
        std::fs::write(&file, "0 press 1 apple leftmeta\n10 press 1 apple c\n").unwrap();
        std::fs::write(
            &config,
            "[profile.default]\nrules = [{ when = \"command+c\", then = \"x\" }]\n",
        )
        .unwrap();
        let plain = replay_file(&file, None).unwrap();
        let with_config = replay_file(&file, Some(&config)).unwrap();
        assert_eq!(plain[1], "10 press 1 apple c -> +c SYN");
        assert_eq!(with_config[1], "10 press 1 apple c -> -leftctrl +x SYN");
    }

    #[test]
    fn the_pause_chord_renders_its_release_and_status() {
        let steps = parse("0 press 1 apple leftshift\n1 press 1 apple rightshift\n2 press 1 apple esc\n3 resume\n").unwrap();
        let (profiles, _) = Layers::builtin_only().resolve();
        let lines = run(steps, &profiles);
        assert_eq!(
            lines[2],
            "2 press 1 apple esc -> -leftshift SYN; release-grabs; status paused"
        );
        assert_eq!(lines[3], "3 resume -> status running");
    }

    #[test]
    fn a_bad_script_or_a_missing_file_is_an_error_naming_the_file() {
        let dir = Scratch::new("replay-bad");
        let file = dir.path().join("bad.txt");
        std::fs::write(&file, "0 resume\n1 press 1 apple nope\n").unwrap();
        let err = replay_file(&file, None).unwrap_err();
        assert!(err.contains("bad.txt") && err.contains("line 2"), "{err}");
        let missing = replay_file(&dir.path().join("none.txt"), None).unwrap_err();
        assert!(missing.contains("none.txt"), "{missing}");
        std::fs::write(&file, "0 resume\n").unwrap();
        let no_config = replay_file(&file, Some(&dir.path().join("none.toml"))).unwrap_err();
        assert!(no_config.contains("none.toml"), "{no_config}");
    }

    #[test]
    fn render_table() {
        use crate::{Key, Logical};
        let key = |name: &str, edge| OutEvent::Key {
            key: Logical(Key::named(name).unwrap()),
            edge,
        };
        let cases: &[(&str, Vec<RemapOut>, &str)] = &[
            ("nothing", vec![], ""),
            (
                "press release repeat",
                vec![RemapOut::Write(vec![
                    key("a", Edge::Press),
                    key("a", Edge::Repeat),
                    key("a", Edge::Release),
                    OutEvent::Sync,
                ])],
                "+a ~a -a SYN",
            ),
            (
                "several outputs",
                vec![
                    RemapOut::Write(vec![OutEvent::Sync]),
                    RemapOut::ReleaseGrabs,
                    RemapOut::Status(CoreStatus::Paused),
                    RemapOut::Status(CoreStatus::Running),
                ],
                "SYN; release-grabs; status paused; status running",
            ),
        ];
        for (name, outs, want) in cases {
            assert_eq!(&render(outs), want, "{name}");
        }
    }
}
