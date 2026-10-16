//! Judging a reading of the keymap file against the last good copy. Pure.
//!
//! chordkit reads the file leniently: a bad line is reported and skipped. keycap adds one rule
//! on top: a file that is unusable, or whose every line is bad, is a bad edit (half a save, a
//! pasted error), so the last good copy stays in force and the problem is reported.

use chordkit::{Desktop, Keymap, OverrideProblem, Overrides, Platform};
use keycap_protocol::{KeymapProblem, KeymapState};

use super::file::Reading;

/// The platform the file is checked against: ours, whose reserved chords the person may not take.
const OURS: Platform = Platform::Linux {
    desktop: Desktop::Ours,
};

fn problem(line: usize, what: String) -> KeymapProblem {
    KeymapProblem {
        line: u32::try_from(line).unwrap_or(0),
        what,
    }
}

fn plain(found: &OverrideProblem) -> KeymapProblem {
    problem(found.line, found.what.clone())
}

/// Problems reported at once; the rest are summed in one last line. A file of thousands of bad
/// lines would otherwise make a reply no client could read.
const MAX_PROBLEMS: usize = 50;

/// Characters of one problem's text; a bad line can be as long as the file.
const MAX_PROBLEM_CHARS: usize = 200;

/// The state after `reading`, given the state in force before it, with the problems within
/// bounds so the state always fits one packet.
pub fn settle(previous: &KeymapState, reading: Reading) -> KeymapState {
    let state = settle_unbounded(previous, reading);
    KeymapState {
        problems: bounded(state.problems),
        ..state
    }
}

fn bounded(problems: Vec<KeymapProblem>) -> Vec<KeymapProblem> {
    let more = problems.len().saturating_sub(MAX_PROBLEMS);
    let mut kept: Vec<KeymapProblem> = problems
        .into_iter()
        .take(MAX_PROBLEMS)
        .map(|p| KeymapProblem {
            what: p.what.chars().take(MAX_PROBLEM_CHARS).collect(),
            ..p
        })
        .collect();
    if more > 0 {
        kept.push(problem(0, format!("{more} more lines have problems.")));
    }
    kept
}

fn settle_unbounded(previous: &KeymapState, reading: Reading) -> KeymapState {
    match reading {
        Reading::Missing => KeymapState::default(),
        Reading::Unusable(why) => keep(
            previous,
            vec![problem(
                0,
                format!("The keymap file couldn't be used ({why}), so the last good one stays."),
            )],
        ),
        Reading::Text(text) => {
            let (overrides, mut problems) = Overrides::parse(&text);
            if overrides.entries().is_empty() && !problems.is_empty() {
                let mut kept = vec![problem(
                    0,
                    "No line in the keymap file could be read, so the last good one stays."
                        .to_owned(),
                )];
                kept.extend(problems.iter().map(plain));
                return keep(previous, kept);
            }
            let (_, refused) = Keymap::conventional(OURS).with_overrides(&overrides);
            problems.extend(refused);
            KeymapState {
                overrides: overrides.render(),
                problems: problems.iter().map(plain).collect(),
            }
        }
    }
}

fn keep(previous: &KeymapState, problems: Vec<KeymapProblem>) -> KeymapState {
    KeymapState {
        overrides: previous.overrides.clone(),
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Reading {
        Reading::Text(t.to_owned())
    }

    fn good() -> KeymapState {
        settle(&KeymapState::default(), text("copy = Primary+Alt+C\n"))
    }

    #[test]
    fn a_clean_file_is_served_in_chordkits_canonical_form_with_no_problems() {
        let state = good();
        assert_eq!(state.problems, []);
        let (round, bad) = Overrides::parse(&state.overrides);
        assert_eq!(bad, []);
        assert_eq!(round.entries().len(), 1);
    }

    #[test]
    fn thousands_of_bad_lines_are_reported_as_fifty_and_a_count() {
        let lines: String = (0..3000)
            .map(|n| format!("bogus{n} = Primary+Q\n"))
            .collect();
        let state = settle(&good(), text(&format!("copy = Primary+Alt+C\n{lines}")));
        assert_eq!(state.problems.len(), MAX_PROBLEMS + 1);
        let last = state.problems.last().expect("a last line");
        assert!(last.what.ends_with("more lines have problems."), "{last:?}");
        let size = keycap_protocol::encode(&keycap_protocol::ToClient::Keymap(state)).len();
        assert!(size < keycap_protocol::MAX_PACKET, "{size}");
    }

    #[test]
    fn one_very_long_bad_line_is_cut_in_its_report() {
        let long = "x".repeat(10_000);
        let state = settle(
            &good(),
            text(&format!("copy = Primary+Alt+C\n{long} = Primary+Q\n")),
        );
        assert!(
            state
                .problems
                .iter()
                .all(|p| p.what.chars().count() <= MAX_PROBLEM_CHARS)
        );
    }

    #[test]
    fn a_missing_file_clears_the_overrides() {
        assert_eq!(settle(&good(), Reading::Missing), KeymapState::default());
    }

    #[test]
    fn bad_lines_are_skipped_and_reported_with_their_line_while_the_rest_applies() {
        let state = settle(
            &KeymapState::default(),
            text("# mine\ncopy = Primary+Alt+C\nteleport = Primary+T\nredo = none\n"),
        );
        assert_eq!(state.problems.len(), 1, "{:?}", state.problems);
        assert_eq!(state.problems[0].line, 3);
        assert!(
            state.problems[0].what.contains("teleport"),
            "{:?}",
            state.problems
        );
        let (kept, _) = Overrides::parse(&state.overrides);
        assert_eq!(kept.entries().len(), 2);
    }

    #[test]
    fn a_chord_the_desktop_keeps_is_reported_in_plain_words() {
        let state = settle(
            &KeymapState::default(),
            text("copy = Ctrl+1\nundo = Primary+Alt+Z\n"),
        );
        assert_eq!(state.problems.len(), 1, "{:?}", state.problems);
        assert_eq!(state.problems[0].line, 1);
        assert!(
            state.problems[0].what.contains("Space 1"),
            "{:?}",
            state.problems
        );
    }

    #[test]
    fn a_file_with_no_readable_line_keeps_the_last_good_copy() {
        let before = good();
        let after = settle(&before, text("<<<<<<< HEAD\nnot a keymap\n"));
        assert_eq!(after.overrides, before.overrides);
        assert!(after.problems.len() >= 2, "{:?}", after.problems);
        assert_eq!(after.problems[0].line, 0);
    }

    #[test]
    fn an_unusable_file_keeps_the_last_good_copy_and_says_why() {
        let before = good();
        let after = settle(&before, Reading::Unusable("it is not plain text".into()));
        assert_eq!(after.overrides, before.overrides);
        assert_eq!(after.problems.len(), 1);
        assert!(after.problems[0].what.contains("not plain text"));
    }

    #[test]
    fn an_empty_or_comment_only_file_is_a_clean_empty_keymap() {
        for t in ["", "\n\n", "# nothing yet\n"] {
            assert_eq!(settle(&good(), text(t)), KeymapState::default(), "{t:?}");
        }
    }

    #[test]
    fn a_fixed_file_replaces_the_kept_copy_and_clears_the_problems() {
        let before = good();
        let broken = settle(&before, text("???\n"));
        assert!(!broken.problems.is_empty());
        let fixed = settle(&broken, text("paste = Primary+Alt+V\n"));
        assert_eq!(fixed.problems, []);
        assert_ne!(fixed.overrides, before.overrides);
    }
}
