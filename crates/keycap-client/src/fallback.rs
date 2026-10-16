//! A keymap file read straight from disk: the person's own, and the machine's when no daemon
//! answers.

use crate::bounded::{FileText, read_text};
use chordkit::Overrides;
use keycap_protocol::KEYMAP_MAX_BYTES;
use std::path::Path;

/// The overrides in `path` and what was wrong with them, in plain words. A missing file is no
/// overrides; one that cannot be used is no overrides and one problem.
pub(crate) fn read(path: &Path) -> (Overrides, Vec<String>) {
    match read_text(path, KEYMAP_MAX_BYTES) {
        FileText::Text(text) => {
            let (overrides, problems) = Overrides::parse(&text);
            (
                overrides,
                problems.iter().map(ToString::to_string).collect(),
            )
        }
        FileText::Missing => (Overrides::default(), Vec::new()),
        FileText::Unusable(why) => (
            Overrides::default(),
            vec![format!("The keymap file couldn't be used ({why}).")],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Scratch;

    #[test]
    fn missing_file_is_no_overrides() {
        let dir = Scratch::new("fallback-missing");
        let (overrides, problems) = read(&dir.path().join("keymap"));
        assert_eq!((overrides.entries().len(), problems), (0, vec![]));
    }

    #[test]
    fn good_and_bad_lines_are_read_leniently_with_the_line_in_the_problem() {
        let dir = Scratch::new("fallback-lines");
        let path = dir.path().join("keymap");
        std::fs::write(&path, "copy = Primary+Alt+C\nteleport = F5\n").unwrap();
        let (overrides, problems) = read(&path);
        assert_eq!(overrides.entries().len(), 1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("line 2: "), "{problems:?}");
    }

    #[test]
    fn a_file_that_is_not_text_is_one_problem() {
        let dir = Scratch::new("fallback-binary");
        let path = dir.path().join("keymap");
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        let (overrides, problems) = read(&path);
        assert_eq!(overrides.entries().len(), 0);
        assert_eq!(problems.len(), 1);
    }
}
