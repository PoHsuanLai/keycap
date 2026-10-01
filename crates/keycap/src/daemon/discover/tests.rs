use super::caps::{Bits, Caps};
use super::classify::{Eligibility, Origin, classify};
use super::hotplug::{concerns_us, hotplug_of};
use super::*;
use crate::daemon::events::{EV_ABS, EV_KEY, EV_REL, EV_SYN, REL_X, REL_Y};
use crate::daemon::pointer::twin_name;
use crate::daemon::scratch::Scratch;
use crate::daemon::uinput;
use crate::key::Key;
use notify::EventKind;
use notify::event::{CreateKind, MetadataKind, ModifyKind, RemoveKind, RenameMode};
use std::path::{Path, PathBuf};

/// A sysfs mask with the given bits set, as the kernel prints it: 64-bit hex words, most
/// significant first.
fn mask(bits: &[u16]) -> String {
    let words = bits.iter().max().map_or(1, |&m| usize::from(m) / 64 + 1);
    (0..words)
        .rev()
        .map(|w| {
            let word = bits
                .iter()
                .filter(|&&b| usize::from(b) / 64 == w)
                .fold(0u64, |acc, &b| acc | 1 << (b % 64));
            format!("{word:x}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn codes(names: &[&str]) -> Vec<u16> {
    names
        .iter()
        .map(|n| Key::named(n).expect("a key name").code())
        .collect()
}

const LETTERS: &[&str] = &[
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

fn typing_keys() -> Vec<u16> {
    let mut keys = codes(LETTERS);
    keys.extend(codes(&["space", "enter", "leftshift", "leftmeta"]));
    keys
}

fn caps(ev: &[u16], key: &[u16], rel: &[u16]) -> Caps {
    Caps {
        ev: Bits::parse(&mask(ev)),
        key: Bits::parse(&mask(key)),
        rel: Bits::parse(&mask(rel)),
    }
}

fn identity(name: &str, vendor: u16, product: u16) -> Identity {
    Identity {
        name: name.to_owned(),
        vendor,
        product,
        path: PathBuf::from("/dev/input/event0"),
    }
}

#[test]
fn bits_parse_words_most_significant_first() {
    let b = Bits::parse("1 8000000000000002");
    assert!(b.has(1) && b.has(63), "low word");
    assert!(b.has(64), "high word's lowest bit");
    assert!(!b.has(0) && !b.has(2) && !b.has(65));
    assert!(!Bits::parse("").has(0));
    assert!(!Bits::parse("zz").has(0), "an unreadable word is empty");
}

#[test]
fn bits_from_ioctl_bytes_are_little_endian_words_least_significant_first() {
    let b = Bits::from_bytes(&[0b10, 0, 0, 0, 0, 0, 0, 0x80, 0x01]);
    assert!(b.has(1) && b.has(63), "first word");
    assert!(b.has(64), "second word's lowest bit");
    assert!(!b.has(0) && !b.has(2) && !b.has(65));
    assert!(!Bits::from_bytes(&[]).has(0));
    assert_eq!(
        Bits::from_bytes(&[1, 2, 3]),
        Bits::from_bytes(&[1, 2, 3, 0]),
        "a short tail pads"
    );
}

#[test]
fn bits_round_trip_through_the_test_mask() {
    let bits = [0u16, 1, 30, 63, 64, 0x110, 0x2ff];
    let parsed = Bits::parse(&mask(&bits));
    for n in 0..0x300u32 {
        let want = bits.iter().any(|&b| u32::from(b) == n);
        assert_eq!(parsed.has(n), want, "bit {n}");
    }
}

#[test]
fn classify_table() {
    let ev_kb = [EV_SYN, EV_KEY];
    let ev_combo = [EV_SYN, EV_KEY, EV_REL];
    let apple = identity("Apple Inc. Magic Keyboard", 0x004c, 0x0267);
    let mut combo_keys = typing_keys();
    combo_keys.extend([0x110, 0x111]);
    let mut no_space = typing_keys();
    no_space.retain(|&c| c != Key::named("space").unwrap().code());
    let mut no_z = typing_keys();
    no_z.retain(|&c| c != Key::named("z").unwrap().code());

    let cases: Vec<(&str, Caps, Identity, Origin, Eligibility)> = vec![
        (
            "a plain keyboard",
            caps(&ev_kb, &typing_keys(), &[]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::Keyboard,
        ),
        (
            "a keyboard with a trackpad",
            caps(&ev_combo, &combo_keys, &[REL_X, REL_Y]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::KeyboardAndPointer,
        ),
        (
            "EV_REL without REL_X is not a pointer",
            caps(&ev_combo, &typing_keys(), &[REL_Y]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::Keyboard,
        ),
        (
            "REL_X without EV_REL is not a pointer",
            caps(&ev_kb, &typing_keys(), &[REL_X]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::Keyboard,
        ),
        (
            "a power button",
            caps(&ev_kb, &codes(&["power"]), &[]),
            identity("Power Button", 0, 1),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "a lid switch",
            caps(&[EV_SYN, 0x05], &[], &[]),
            identity("Lid Switch", 0, 5),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "a media remote",
            caps(
                &ev_kb,
                &codes(&["playpause", "volumeup", "mute", "nextsong"]),
                &[],
            ),
            identity("Consumer Control", 0x046d, 0xc52b),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "a mouse with a few keys",
            caps(
                &ev_combo,
                &[0x110, 0x111, 0x112, 0x113, 0x114],
                &[REL_X, REL_Y],
            ),
            identity("Logitech Mouse", 0x046d, 0xc077),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "letters but no space bar",
            caps(&ev_kb, &no_space, &[]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "a missing letter",
            caps(&ev_kb, &no_z, &[]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "keys without EV_KEY in the event mask",
            caps(&[EV_SYN], &typing_keys(), &[]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "our own virtual keyboard, by name",
            caps(&ev_combo, &combo_keys, &[REL_X, REL_Y]),
            identity(uinput::NAME, 0x1234, 0x5678),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "our own virtual keyboard, by vendor and product",
            caps(&ev_kb, &typing_keys(), &[]),
            identity("renamed", uinput::VENDOR, uinput::PRODUCT),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "palmrest's virtual keys",
            caps(&ev_kb, &typing_keys(), &[]),
            identity("palmrest keys", 0x0001, 0x0003),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "another remapper's virtual keyboard",
            caps(&ev_kb, &typing_keys(), &[]),
            identity("kmonad", 0x1234, 0x5678),
            Origin::Virtual,
            Eligibility::NotKeyboard,
        ),
        (
            "a virtual keyboard-and-pointer",
            caps(&ev_combo, &combo_keys, &[REL_X, REL_Y]),
            identity("remote desktop input", 0, 0),
            Origin::Virtual,
            Eligibility::NotKeyboard,
        ),
        (
            "a keyboard with a touchpad: absolute axes make it a pointer",
            caps(&[EV_SYN, EV_KEY, EV_ABS], &combo_keys, &[]),
            apple.clone(),
            Origin::Hardware,
            Eligibility::KeyboardAndPointer,
        ),
        (
            "our own pointer twin, a virtual device with the keys and axes of its original",
            caps(&[EV_SYN, EV_KEY, EV_ABS], &combo_keys, &[]),
            identity(&twin_name("Apple Inc. Magic Keyboard"), 0x004c, 0x0267),
            Origin::Virtual,
            Eligibility::NotKeyboard,
        ),
        (
            "our own pointer twin, even where it registers as hardware",
            caps(&[EV_SYN, EV_KEY, EV_ABS], &combo_keys, &[]),
            identity(&twin_name("Apple Inc. Magic Keyboard"), 0x004c, 0x0267),
            Origin::Hardware,
            Eligibility::NotKeyboard,
        ),
        (
            "same product, other vendor, is not ours",
            caps(&ev_kb, &typing_keys(), &[]),
            identity("Some Keyboard", 0x1111, uinput::PRODUCT),
            Origin::Hardware,
            Eligibility::Keyboard,
        ),
    ];
    for (name, c, id, origin, want) in cases {
        assert_eq!(classify(&c, &id, origin), want, "{name}");
    }
}

#[test]
fn origin_is_read_off_the_resolved_sysfs_path() {
    const CASES: &[(&str, &str, Origin)] = &[
        (
            "a uinput device",
            "/sys/devices/virtual/input/input23/event7",
            Origin::Virtual,
        ),
        (
            "a usb keyboard",
            "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0/0003:05AC:0267.0004/input/input21/event5",
            Origin::Hardware,
        ),
        (
            "a bluetooth keyboard through uhid",
            "/sys/devices/virtual/misc/uhid/0005:05AC:0267.0008/input/input30/event9",
            Origin::Hardware,
        ),
        (
            "a ps/2 keyboard",
            "/sys/devices/platform/i8042/serio0/input/input3/event3",
            Origin::Hardware,
        ),
        (
            "a name that only starts like the directory",
            "/sys/devices/virtual/input-ish/input1/event1",
            Origin::Hardware,
        ),
        (
            "the directory under a relocated sysfs root",
            "/mnt/sys/devices/virtual/input/input2/event0",
            Origin::Virtual,
        ),
    ];
    for &(name, path, want) in CASES {
        assert_eq!(Origin::of_sysfs(Path::new(path)), want, "{name}");
    }
}

/// One node of a fake `/sys/class/input`.
fn node(sys: &Path, entry: &str, name: &str, ids: (u16, u16), caps: (&[u16], &[u16], &[u16])) {
    let d = sys.join(entry);
    std::fs::create_dir_all(d.join("device/capabilities")).unwrap();
    std::fs::create_dir_all(d.join("device/id")).unwrap();
    std::fs::write(d.join("device/name"), format!("{name}\n")).unwrap();
    std::fs::write(d.join("device/id/vendor"), format!("{:04x}\n", ids.0)).unwrap();
    std::fs::write(d.join("device/id/product"), format!("{:04x}\n", ids.1)).unwrap();
    for (f, bits) in [("ev", caps.0), ("key", caps.1), ("rel", caps.2)] {
        std::fs::write(d.join("device/capabilities").join(f), mask(bits) + "\n").unwrap();
    }
}

#[test]
fn scan_reads_a_sysfs_tree_and_keeps_only_eligible_keyboards() {
    let root = Scratch::new("scan");
    let (sys, dev) = (root.path().join("sys"), Path::new("/fake/dev/input"));
    let kb_ev = [EV_SYN, EV_KEY];
    let combo_ev = [EV_SYN, EV_KEY, EV_REL];
    node(
        &sys,
        "event0",
        "AT Keyboard",
        (0x0001, 0x0001),
        (&kb_ev, &typing_keys(), &[]),
    );
    node(
        &sys,
        "event1",
        "Power Button",
        (0, 1),
        (&kb_ev, &codes(&["power"]), &[]),
    );
    node(
        &sys,
        "event2",
        uinput::NAME,
        (uinput::VENDOR, uinput::PRODUCT),
        (&combo_ev, &typing_keys(), &[REL_X, REL_Y]),
    );
    node(
        &sys,
        "event10",
        "Apple Combo",
        (0x004c, 0x0267),
        (&combo_ev, &typing_keys(), &[REL_X, REL_Y]),
    );
    // A uinput keyboard that looks like a perfect keyboard: the class entry links into
    // `devices/virtual/input`, as the kernel lays it out.
    let virtual_dir = root.path().join("devices/virtual/input/input9");
    node(
        &virtual_dir,
        "event3",
        "other remapper output",
        (0x1234, 0x5678),
        (&kb_ev, &typing_keys(), &[]),
    );
    std::os::unix::fs::symlink(virtual_dir.join("event3"), sys.join("event3")).unwrap();
    node(
        &sys,
        "mouse0",
        "Legacy mouse",
        (0, 0),
        (&kb_ev, &typing_keys(), &[]),
    );
    node(
        &sys,
        "eventX",
        "Not a node",
        (0, 0),
        (&kb_ev, &typing_keys(), &[]),
    );
    std::fs::create_dir_all(sys.join("event7")).unwrap();

    let found = scan(&sys, dev);
    let summary: Vec<(&str, &Path, Eligibility, u16)> = found
        .iter()
        .map(|c| {
            (
                c.identity.name.as_str(),
                c.identity.path.as_path(),
                c.eligibility,
                c.identity.vendor,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "AT Keyboard",
                Path::new("/fake/dev/input/event0"),
                Eligibility::Keyboard,
                0x0001
            ),
            (
                "Apple Combo",
                Path::new("/fake/dev/input/event10"),
                Eligibility::KeyboardAndPointer,
                0x004c
            ),
        ]
    );
}

#[test]
fn scan_of_a_missing_root_is_empty() {
    let root = Scratch::new("scan-missing");
    assert_eq!(scan(&root.path().join("nope"), Path::new("/dev/input")), []);
}

#[test]
fn only_event_nodes_concern_us() {
    const CASES: &[(&str, bool)] = &[
        ("/dev/input/event0", true),
        ("/dev/input/event17", true),
        ("/sys/class/input/event3", true),
        ("/dev/input/event", false),
        ("/dev/input/eventX", false),
        ("/dev/input/event1a", false),
        ("/dev/input/mouse0", false),
        ("/dev/input/mice", false),
        ("/dev/input/by-id", false),
        ("/dev/input/by-path/pci-0000:00:14.0-event-kbd", false),
        ("/dev/input/", false),
    ];
    for &(path, want) in CASES {
        assert_eq!(concerns_us(Path::new(path)), want, "{path}");
    }
}

#[test]
fn hotplug_table() {
    let node = PathBuf::from("/dev/input/event5");
    let other = Path::new("/dev/input/mouse0");
    let appeared = Some(Hotplug::Appeared(node.clone()));
    let gone = Some(Hotplug::Gone(node.clone()));
    let cases: Vec<(&str, EventKind, &Path, Option<Hotplug>)> = vec![
        (
            "created",
            EventKind::Create(CreateKind::File),
            &node,
            appeared.clone(),
        ),
        (
            "permissions opened up",
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Permissions)),
            &node,
            appeared.clone(),
        ),
        (
            "renamed in",
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            &node,
            appeared,
        ),
        (
            "removed",
            EventKind::Remove(RemoveKind::File),
            &node,
            gone.clone(),
        ),
        (
            "renamed away",
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            &node,
            gone,
        ),
        (
            "data written",
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            &node,
            None,
        ),
        (
            "opened",
            EventKind::Access(notify::event::AccessKind::Any),
            &node,
            None,
        ),
        (
            "another name",
            EventKind::Create(CreateKind::File),
            other,
            None,
        ),
    ];
    for (name, kind, path, want) in cases {
        assert_eq!(hotplug_of(&kind, path), want, "{name}");
    }
}

#[test]
fn the_watch_reports_event_nodes_and_ignores_other_names() {
    let dir = Scratch::new("hotplug");
    let (tx, rx) = std::sync::mpsc::channel();
    let _watcher = spawn(dir.path(), tx).unwrap();
    std::fs::write(dir.path().join("mice"), "").unwrap();
    let node = dir.path().join("event4");
    std::fs::write(&node, "").unwrap();
    let wait = std::time::Duration::from_secs(5);
    assert_eq!(rx.recv_timeout(wait), Ok(Hotplug::Appeared(node.clone())));
    std::fs::remove_file(&node).unwrap();
    let gone =
        std::iter::from_fn(|| rx.recv_timeout(wait).ok()).find(|h| matches!(h, Hotplug::Gone(_)));
    assert_eq!(gone, Some(Hotplug::Gone(node)));
}
