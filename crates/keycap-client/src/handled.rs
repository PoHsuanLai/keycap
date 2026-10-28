//! Which standard actions our desktop handles, and who carries each out.
//!
//! One table for the desktop: detent lists only these actions on its Shortcuts page, and
//! casement's tests check that the actions it binds are exactly these. An action not named here
//! belongs to apps (copy, save, find, ...).

use chordkit::StandardAction;
use shell_verbs::ShellVerb;

/// Who carries out a handled action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    /// sill, the shell, through this verb.
    Shell(ShellVerb),
    /// The compositor itself.
    Compositor,
}

type Row = (StandardAction, Handler);

const C: Handler = Handler::Compositor;

const fn sh(verb: ShellVerb) -> Handler {
    Handler::Shell(verb)
}

/// The handled actions, except the Space numbers (see [`handler`]).
const ROWS: &[Row] = &[
    (StandardAction::ForceQuit, sh(ShellVerb::ForceQuitToggle)),
    (StandardAction::MissionControl, C),
    (StandardAction::FullScreen, C),
    (StandardAction::SpaceLeft, C),
    (StandardAction::SpaceRight, C),
    (StandardAction::Launcher, sh(ShellVerb::LauncherToggle)),
    (StandardAction::NextApp, sh(ShellVerb::SwitcherNext)),
    (StandardAction::NextWindow, sh(ShellVerb::NextWindow)),
    (StandardAction::ToggleDock, sh(ShellVerb::DockToggleHide)),
    (StandardAction::LockScreen, sh(ShellVerb::Lock)),
    (
        StandardAction::EmojiAndSymbols,
        sh(ShellVerb::EmojiAndSymbols),
    ),
    (
        StandardAction::NextInputSource,
        sh(ShellVerb::NextInputSource),
    ),
    (StandardAction::PreviousApp, sh(ShellVerb::SwitcherPrev)),
    (
        StandardAction::ScreenshotScreen,
        sh(ShellVerb::ScreenshotScreen),
    ),
    (
        StandardAction::ScreenshotSelection,
        sh(ShellVerb::ScreenshotArea),
    ),
    (
        StandardAction::ScreenshotTools,
        sh(ShellVerb::ScreenshotArea),
    ),
    (StandardAction::LogOut, sh(ShellVerb::PowerMenuOpen)),
    (StandardAction::TileLeft, C),
    (StandardAction::TileRight, C),
    (StandardAction::TileTop, C),
    (StandardAction::TileBottom, C),
    (StandardAction::TileTopLeft, C),
    (StandardAction::TileTopRight, C),
    (StandardAction::TileBottomLeft, C),
    (StandardAction::TileBottomRight, C),
    (StandardAction::Fill, C),
    (StandardAction::Center, C),
    (StandardAction::ReturnToPreviousSize, C),
];

/// Who handles `action`, or `None` when it belongs to apps. Every Space number is the
/// compositor's.
pub fn handler(action: StandardAction) -> Option<Handler> {
    match action {
        StandardAction::SwitchToSpace(_) => Some(Handler::Compositor),
        other => ROWS
            .iter()
            .find(|(named, _)| *named == other)
            .map(|(_, handler)| *handler),
    }
}

/// Every handled action with its handler, in `StandardAction::all` order.
pub fn handled_actions() -> impl Iterator<Item = (StandardAction, Handler)> {
    StandardAction::all().filter_map(|action| handler(action).map(|handler| (action, handler)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_action_appears_twice() {
        let mut seen: Vec<StandardAction> = ROWS.iter().map(|(action, _)| *action).collect();
        let before = seen.len();
        seen.sort_by_key(|action| action.slug());
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn every_shell_row_names_a_known_verb() {
        for (action, handler) in ROWS {
            if let Handler::Shell(verb) = handler {
                assert!(shell_verbs::ALL.contains(verb), "{}", action.slug());
            }
        }
    }

    #[test]
    fn every_row_and_every_space_is_listed() {
        let listed: Vec<StandardAction> = handled_actions().map(|(action, _)| action).collect();
        assert!(ROWS.iter().all(|(action, _)| listed.contains(action)));
        assert_eq!(listed.len(), ROWS.len() + 9);
    }
}
