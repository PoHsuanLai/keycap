use keycap_protocol::{Mode, ProfileName};

use super::*;
use crate::config::{ConfigLayer, ConfigSource, resolve};
use crate::event::{DeviceId, Edge, OutEvent, Physical};
use crate::key::Key;
use crate::layout::Layout;
use crate::profile::Profiles;
use crate::role::{Role, Roles};
use crate::time::Mono;

const CONFIG: &str = r#"
[profile.default]
modmap = { command = "ctrl", option = "alt", control = "super" }
rules = [
  { when = "command+x", then = [] },
  { when = "command+s", then = ["a", "b"] },
  { when = "command+m", then = "alt+z" },
  { when = "command+tab", then = "alt+tab", hold = "while_held" },
  { when = "shift+y", then = "alt+k", hold = "while_held" },
]
"#;

fn profiles() -> Profiles {
    let (layer, warnings) = ConfigLayer::parse(CONFIG, ConfigSource::Builtin);
    assert!(warnings.is_empty(), "{warnings:?}");
    resolve(&[layer]).0
}

fn key(name: &str) -> Physical {
    Physical(Key::named(name).expect("a key name"))
}

fn input(dev: u32, name: &str, edge: Edge) -> RemapInput {
    RemapInput::Key {
        device: DeviceId(dev),
        layout: Layout::Apple,
        key: key(name),
        edge,
        at: Mono::default(),
    }
}

fn render(outs: &[RemapOut]) -> String {
    let event = |e: &OutEvent| match e {
        OutEvent::Key { key, edge } => {
            let sign = if *edge == Edge::Press { '+' } else { '-' };
            format!("{sign}{}", key.0.name())
        }
        OutEvent::Sync => "SYN".to_owned(),
    };
    let out = |o: &RemapOut| match o {
        RemapOut::Write(events) => events.iter().map(event).collect::<Vec<_>>().join(" "),
        RemapOut::ReleaseGrabs => "GRABS-OFF".to_owned(),
        RemapOut::Status(status) => format!("{status:?}"),
    };
    outs.iter().map(out).collect::<Vec<_>>().join("; ")
}

/// Run `script` (`+name` press, `-name` release, `~name` repeat, optional `dev:` prefix) and
/// return the final state with the rendering of each step.
fn run(script: &[&str]) -> (Remapper, Vec<String>) {
    let profiles = profiles();
    let mut state = Remapper::new();
    let mut rendered = Vec::new();
    for step in script {
        let (dev, rest) = step.split_once(':').map_or((0, *step), |(d, r)| {
            (d.parse().expect("a device number"), r)
        });
        let (edge, name) = match rest.split_at(1) {
            ("+", name) => (Edge::Press, name),
            ("-", name) => (Edge::Release, name),
            (_, name) => (Edge::Repeat, name),
        };
        let (next, outs) = state.step(input(dev, name, edge), &profiles);
        state = next;
        rendered.push(render(&outs));
    }
    (state, rendered)
}

#[test]
fn swallow_writes_nothing_on_press_or_release() {
    let (_, out) = run(&["+leftmeta", "+x", "-x", "-leftmeta"]);
    assert_eq!(out, ["+leftctrl SYN", "", "", "-leftctrl SYN"]);
}

#[test]
fn sequence_taps_each_stroke_in_one_write_and_leaves_nothing_held() {
    let (state, out) = run(&["+leftmeta", "+s", "-s"]);
    assert_eq!(out[1], "-leftctrl +a -a +b -b +leftctrl SYN");
    assert_eq!(out[2], "");
    assert!(state.held.is_held((DeviceId(0), key("leftmeta"))));
    assert!(!state.held.is_held((DeviceId(0), key("s"))));
}

#[test]
fn restore_map_lifts_unwanted_modifiers_and_puts_them_back() {
    let (_, out) = run(&["+leftmeta", "+m", "-m"]);
    assert_eq!(out[1], "-leftctrl +leftalt +z SYN");
    assert_eq!(out[2], "-z -leftalt +leftctrl SYN");
}

#[test]
fn while_held_modifiers_belong_to_the_command_key() {
    let (_, out) = run(&["+leftmeta", "+tab", "-tab", "+tab", "-leftmeta", "-tab"]);
    assert_eq!(
        out,
        [
            "+leftctrl SYN",
            "-leftctrl +leftalt +tab SYN",
            "-tab SYN",
            "+tab SYN",
            "-leftalt SYN",
            "-tab SYN",
        ]
    );
}

#[test]
fn while_held_belongs_to_the_lowest_command_key_and_the_other_restores_control() {
    let (_, out) = run(&[
        "+rightmeta",
        "+leftmeta",
        "+tab",
        "-tab",
        "-leftmeta",
        "-rightmeta",
    ]);
    assert_eq!(
        out,
        [
            "+leftctrl SYN",
            "",
            "-leftctrl +leftalt +tab SYN",
            "-tab SYN",
            "-leftalt +leftctrl SYN",
            "-leftctrl SYN",
        ]
    );
}

#[test]
fn while_held_without_a_non_shift_role_acts_as_restore() {
    let (_, out) = run(&["+leftshift", "+y", "-y", "-leftshift"]);
    assert_eq!(
        out,
        [
            "+leftshift SYN",
            "-leftshift +leftalt +k SYN",
            "-k -leftalt +leftshift SYN",
            "-leftshift SYN",
        ]
    );
}

#[test]
fn a_press_of_a_held_key_releases_its_old_write_first() {
    let (_, out) = run(&["+a", "+a"]);
    assert_eq!(out[1], "-a +a SYN");
}

#[test]
fn repeats_and_unmatched_releases_are_silent() {
    let (_, out) = run(&["~a", "-a", "+a", "~a", "-a"]);
    assert_eq!(out, ["", "", "+a SYN", "", "-a SYN"]);
}

#[test]
fn held_roles_follow_what_is_held() {
    let (state, _) = run(&["+leftmeta", "+rightshift", "1:+leftalt"]);
    let want: Roles = [Role::Command, Role::Shift, Role::Option]
        .into_iter()
        .collect();
    assert_eq!(state.held_roles(), want);
    let (state, _) = run(&["+leftmeta", "+rightshift", "1:+leftalt", "-leftmeta"]);
    let want: Roles = [Role::Shift, Role::Option].into_iter().collect();
    assert_eq!(state.held_roles(), want);
}

#[test]
fn device_gone_and_release_all_empty_the_held_roles() {
    let profiles = profiles();
    for gone in [RemapInput::DeviceGone(DeviceId(0)), RemapInput::ReleaseAll] {
        let (state, _) = run(&["+leftmeta", "+leftshift"]);
        let (state, outs) = state.step(gone, &profiles);
        assert_eq!(render(&outs), "-leftshift -leftctrl SYN");
        assert_eq!(state.held_roles(), Roles::EMPTY);
        assert!(state.roles.is_empty());
        assert!(state.held.is_empty());
    }
}

#[test]
fn the_remapper_is_paused_by_the_chord_and_running_again_after_resume() {
    let profiles = profiles();
    let (state, out) = run(&["+leftshift", "+rightshift", "+esc"]);
    assert_eq!(out[2], "-leftshift SYN; GRABS-OFF; Paused");
    assert_eq!(state.status(), Mode::Paused);
    let (state, outs) = state.step(RemapInput::Focus(ProfileName::default_profile()), &profiles);
    assert!(outs.is_empty());
    let (state, outs) = state.step(RemapInput::Resume, &profiles);
    assert_eq!(render(&outs), "Running");
    assert_eq!(state.status(), Mode::Running);
}
