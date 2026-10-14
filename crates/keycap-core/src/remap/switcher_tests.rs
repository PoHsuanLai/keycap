//! An app switcher on Super (Cmd+Tab -> Super+Tab) stays open while Command is held: the rules
//! carry `hold = "while_held"`, so Super belongs to Command and Ctrl never flickers between Tabs.
use keycap_protocol::ProfileName;

use super::*;
use crate::config::{ConfigLayer, ConfigSource, resolve};
use crate::event::{DeviceId, Edge, OutEvent, Physical};
use crate::key::Key;
use crate::layout::Layout;
use crate::time::Mono;

const CONFIG: &str = r#"
[profile.default]
modmap = { command = "ctrl", option = "alt", control = "super" }
rules = [
  { when = "command+tab", then = "super+tab", hold = "while_held" },
  { when = "command+shift+tab", then = "super+shift+tab", hold = "while_held" },
  { when = "command+q", then = "alt+f4" },
]
"#;

/// Replay `script` (`+name` press, `-name` release) and render each step's write.
fn run(script: &[&str]) -> Vec<String> {
    let (layer, warnings) = ConfigLayer::parse(CONFIG, ConfigSource::Builtin);
    assert!(warnings.is_empty(), "{warnings:?}");
    let profiles = resolve(&[layer]).0;
    let mut state = Remapper::new();
    let mut rendered = Vec::new();
    for step in script {
        let (edge, name) = match step.split_at(1) {
            ("+", name) => (Edge::Press, name),
            (_, name) => (Edge::Release, name),
        };
        let input = RemapInput::Key {
            device: DeviceId(0),
            layout: Layout::Apple,
            key: Physical(Key::named(name).expect("a key name")),
            edge,
            at: Mono::default(),
        };
        let (next, outs) = state.step(input, &profiles);
        state = next;
        rendered.push(outs.iter().map(render).collect::<Vec<_>>().join("; "));
    }
    assert_eq!(state.focus, ProfileName::default_profile());
    rendered
}

fn render(out: &RemapOut) -> String {
    match out {
        RemapOut::Write(events) => events
            .iter()
            .map(|e| match e {
                OutEvent::Key { key, edge } => {
                    let sign = if *edge == Edge::Press { '+' } else { '-' };
                    format!("{sign}{}", key.0.name())
                }
                OutEvent::Sync => "SYN".to_owned(),
            })
            .collect::<Vec<_>>()
            .join(" "),
        other => format!("{other:?}"),
    }
}

#[test]
fn super_stays_down_across_repeated_tabs_and_goes_up_with_command() {
    let out = run(&["+leftmeta", "+tab", "-tab", "+tab", "-tab", "-leftmeta"]);
    assert_eq!(
        out,
        [
            "+leftctrl SYN",
            "-leftctrl +leftmeta +tab SYN",
            "-tab SYN",
            "+tab SYN",
            "-tab SYN",
            "-leftmeta SYN",
        ]
    );
}

#[test]
fn shift_tab_while_command_is_held_goes_out_under_the_same_super() {
    let out = run(&[
        "+leftmeta",
        "+tab",
        "-tab",
        "+leftshift",
        "+tab",
        "-tab",
        "-leftshift",
        "+tab",
        "-tab",
        "-leftmeta",
    ]);
    assert_eq!(
        out,
        [
            "+leftctrl SYN",
            "-leftctrl +leftmeta +tab SYN",
            "-tab SYN",
            "+leftshift SYN",
            "+tab SYN",
            "-tab SYN",
            "-leftshift SYN",
            "+tab SYN",
            "-tab SYN",
            "-leftmeta SYN",
        ]
    );
}

#[test]
fn command_shift_tab_pressed_together_presses_super_once() {
    let out = run(&[
        "+leftmeta",
        "+leftshift",
        "+tab",
        "-tab",
        "-leftshift",
        "-leftmeta",
    ]);
    assert_eq!(
        out,
        [
            "+leftctrl SYN",
            "+leftshift SYN",
            "-leftctrl +leftmeta +tab SYN",
            "-tab SYN",
            "-leftshift SYN",
            "-leftmeta SYN",
        ]
    );
}

#[test]
fn a_quick_tap_of_the_chord_presses_and_releases_super_once() {
    let out = run(&["+leftmeta", "+tab", "-tab", "-leftmeta"]);
    assert_eq!(
        out,
        [
            "+leftctrl SYN",
            "-leftctrl +leftmeta +tab SYN",
            "-tab SYN",
            "-leftmeta SYN",
        ]
    );
}

#[test]
fn another_command_chord_after_the_release_behaves_as_before() {
    let out = run(&[
        "+leftmeta",
        "+tab",
        "-tab",
        "-leftmeta",
        "+leftmeta",
        "+q",
        "-q",
        "-leftmeta",
    ]);
    assert_eq!(
        &out[4..],
        [
            "+leftctrl SYN",
            "-leftctrl +leftalt +f4 SYN",
            "-f4 -leftalt +leftctrl SYN",
            "-leftctrl SYN",
        ]
    );
}
