//! The bindings our desktop owns itself, as chordkit `SystemBinding` data.
//!
//! chordkit's conventions for our desktop (`Desktop::Ours`) already carry the launcher, the app
//! switcher, the input source, the emoji picker and Ctrl+1 to Ctrl+9 for Spaces, and its
//! reserved table keeps apps off them. This table holds only what the desktop adds to or
//! changes from those conventions: a row replaces the action's chords, so it lists every chord,
//! the convention's first.

use chordkit::{Chord, StandardAction, SystemBinding};

type Row = (StandardAction, &'static [&'static str]);

/// One row per action the desktop binds differently from chordkit's conventions.
const ROWS: &[Row] = &[
    // sill's launcher is also on Ctrl+Alt+Space (dist/cosmic custom shortcuts), where a keyboard
    // has no Command key.
    (StandardAction::Launcher, &["Super+Space", "Ctrl+Alt+Space"]),
];

/// The desktop's own bindings; an unreadable row is left out.
pub fn system_bindings() -> Vec<SystemBinding> {
    ROWS.iter()
        .map(|(action, chords)| SystemBinding {
            action: *action,
            chords: chords
                .iter()
                .filter_map(|text| text.parse::<Chord>().ok())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chordkit::{Context, Convention, ConventionTable, Desktop, Platform};

    const OURS: Platform = Platform::Linux {
        desktop: Desktop::Ours,
    };

    #[test]
    fn every_chord_reads() {
        for (action, texts) in ROWS {
            for text in *texts {
                assert!(text.parse::<Chord>().is_ok(), "{action:?}: {text}");
            }
        }
        let read: usize = system_bindings().iter().map(|b| b.chords.len()).sum();
        let written: usize = ROWS.iter().map(|(_, texts)| texts.len()).sum();
        assert_eq!(read, written);
    }

    #[test]
    fn each_row_starts_with_the_convention_so_the_table_only_adds() {
        let table = ConventionTable::for_platform(OURS);
        for binding in system_bindings() {
            let Some(Convention::Bound(convention)) = table.lookup(binding.action) else {
                panic!("{:?} has no bound convention to extend", binding.action);
            };
            assert_eq!(
                binding.chords.first().copied(),
                convention.resolve(OURS, Context::Normal),
                "{:?}",
                binding.action
            );
        }
    }

    #[test]
    fn no_action_is_listed_twice() {
        let mut slugs: Vec<String> = system_bindings().iter().map(|b| b.action.slug()).collect();
        slugs.sort();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), before);
    }
}
