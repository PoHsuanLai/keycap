//! `keycap replay FILE`: a script through the remapper with the built-in profiles, printing each
//! input and what it writes, the Caps Lock tap of `daemon::caps` included.

use super::script::{Input, Step, parse};
use crate::daemon::caps::{CapsFix, CapsKey};
use crate::daemon::leds::{Led, LedEvent};
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

/// The remapper and the Caps Lock rule, as the reader runs them.
#[derive(Debug, Clone, Default)]
struct Machine {
    remapper: Remapper,
    caps: CapsFix,
}

impl Machine {
    /// The machine after `input`, and what it wrote: the Caps Lock tap first when one is due.
    fn step(self, input: Input, profiles: &Profiles, key: CapsKey) -> (Machine, Vec<RemapOut>) {
        match input {
            Input::Led(LedEvent {
                led: Led::Caps,
                lamp,
            }) => (
                Machine {
                    caps: self.caps.reported(lamp),
                    ..self
                },
                Vec::new(),
            ),
            Input::Led(_) => (self, Vec::new()),
            Input::Remap(input) => {
                let (caps, tap) = self.caps.before(&input, key);
                let (remapper, outs) = self.remapper.step(input, profiles);
                let caps = outs.iter().fold(caps, |caps, out| match out {
                    RemapOut::Write(batch) => caps.wrote(batch),
                    _ => caps,
                });
                let tap = Some(tap).filter(|tap| !tap.is_empty()).map(RemapOut::Write);
                (
                    Machine { remapper, caps },
                    tap.into_iter().chain(outs).collect(),
                )
            }
        }
    }
}

/// One line per step: the input as written, then `->` and what it wrote when it wrote anything.
pub(super) fn run(steps: Vec<Step>, profiles: &Profiles) -> Vec<String> {
    let key = CapsKey::in_profile(profiles.default_profile());
    steps
        .into_iter()
        .scan(Machine::default(), |machine, step| {
            let (next, outs) = std::mem::take(machine).step(step.input, profiles, key);
            *machine = next;
            Some(match outs.as_slice() {
                [] => step.text,
                outs => format!("{} -> {}", step.text, render(outs)),
            })
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
            for w in &warnings {
                eprintln!("keycap: config: {}: {}", w.key, w.problem);
            }
            Layers::builtin_only().with_user(layer)
        }
    };
    let (profiles, warnings) = layers.resolve();
    for w in &warnings {
        eprintln!("keycap: config: {}: {}", w.key, w.problem);
    }
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

    /// The race at a restart: the lamp read Off at the grab, then the desktop set the new
    /// virtual keyboard's Caps Lock LED.
    const RACE: &str = "\
# keycap just grabbed the keyboard; its lamp read off
0 led caps on
10 press 1 apple leftshift
20 press 1 apple a
30 release 1 apple a
40 release 1 apple leftshift
50 led caps off
60 press 1 apple b
70 release 1 apple b
# the user's own Caps Lock, on and off with Shift+Caps Lock
80 press 1 apple leftshift
90 press 1 apple capslock
100 release 1 apple capslock
110 release 1 apple leftshift
120 led caps on
130 press 1 apple c
140 release 1 apple c
150 press 1 apple leftshift
160 press 1 apple capslock
170 release 1 apple capslock
180 release 1 apple leftshift
190 led caps off
200 press 1 apple d
";

    const SWITCH: &str = "[profile.default]\nrules = [\n  { when = \"capslock\", then = \"ctrl+space\" },\n  { when = \"shift+capslock\", then = \"capslock\" },\n]\n";

    #[test]
    fn a_caps_lock_the_desktop_lights_after_the_grab_is_tapped_off_and_a_chosen_one_stays() {
        let dir = Scratch::new("replay-caps");
        let file = dir.path().join("race.txt");
        let config = dir.path().join("user.toml");
        std::fs::write(&file, RACE).unwrap();
        std::fs::write(&config, SWITCH).unwrap();
        let lines = replay_file(&file, Some(&config)).unwrap();
        let tapped: Vec<&str> = lines
            .iter()
            .filter(|line| line.contains("+capslock SYN -capslock SYN;"))
            .map(String::as_str)
            .collect();
        assert_eq!(
            tapped,
            ["20 press 1 apple a -> +capslock SYN -capslock SYN; +a SYN"],
            "{lines:#?}"
        );
        assert_eq!(lines[0], "0 led caps on", "a LED writes nothing");
        assert_eq!(lines[1], "10 press 1 apple leftshift -> +leftshift SYN");
        assert_eq!(lines[13], "130 press 1 apple c -> +c SYN", "{lines:#?}");
        assert_eq!(lines[20], "200 press 1 apple d -> +d SYN", "{lines:#?}");
        let plain = replay_file(&file, None).unwrap();
        assert!(
            plain.iter().all(|line| !line.contains("-capslock SYN;")),
            "Caps Lock not remapped: no tap {plain:#?}"
        );
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
