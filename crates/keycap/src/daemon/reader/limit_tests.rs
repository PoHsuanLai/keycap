//! A settings file over the size limits is refused whole: the earlier settings keep applying and
//! the sender is told why. A recording emitter and a private socket; no node, no device.

use super::tests::Rig;
use crate::daemon::inbox::{ClientId, Request};
use crate::daemon::socket::client::recv;
use crate::{Edge, Key, MAX_PROFILES};
use keycap_protocol::ToClient;

fn code(name: &str) -> u16 {
    Key::named(name).unwrap().code()
}

fn command_c(rig: &mut Rig) -> Vec<(u16, i32)> {
    rig.key("leftmeta", Edge::Press);
    rig.key("c", Edge::Press);
    let written = rig.last_write();
    rig.key("c", Edge::Release);
    rig.key("leftmeta", Edge::Release);
    written
}

#[test]
fn an_oversize_user_config_is_refused_and_the_earlier_one_stays() {
    let mut rig = Rig::new();
    let mine = "[profile.default]\nrules = [{ when = \"command+c\", then = \"x\" }]\n";
    rig.request(Request::UserConfig {
        client: ClientId(0),
        toml: mine.to_owned(),
    });
    assert!(matches!(recv(&rig.client), Some(ToClient::ConfigWarnings(w)) if w.is_empty()));
    assert_eq!(command_c(&mut rig), [(code("leftctrl"), 0), (code("x"), 1)]);

    let huge: String = (0..=MAX_PROFILES)
        .map(|i| format!("[profile.p{i}]\n"))
        .collect();
    rig.request(Request::UserConfig {
        client: ClientId(0),
        toml: huge,
    });
    let Some(ToClient::ConfigWarnings(warnings)) = recv(&rig.client) else {
        panic!("expected ConfigWarnings");
    };
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].problem.contains("earlier settings stay"),
        "{warnings:?}"
    );
    assert_eq!(
        command_c(&mut rig),
        [(code("leftctrl"), 0), (code("x"), 1)],
        "the earlier rule is still in force"
    );
}
