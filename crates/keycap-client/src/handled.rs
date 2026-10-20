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

/// The handled actions, except the Space numbers (see [`handler`]).
const ROWS: &[Row] = &[
    (
        StandardAction::ForceQuit,
        Handler::Shell(ShellVerb::ForceQuitToggle),
    ),
    (StandardAction::MissionControl, Handler::Compositor),
    (StandardAction::FullScreen, Handler::Compositor),
    (StandardAction::SpaceLeft, Handler::Compositor),
    (StandardAction::SpaceRight, Handler::Compositor),
    (
        StandardAction::Launcher,
        Handler::Shell(ShellVerb::LauncherToggle),
    ),
    (
        StandardAction::NextApp,
        Handler::Shell(ShellVerb::SwitcherNext),
    ),
    (
        StandardAction::NextWindow,
        Handler::Shell(ShellVerb::NextWindow),
    ),
    (
        StandardAction::ToggleDock,
        Handler::Shell(ShellVerb::DockToggleHide),
    ),
    (StandardAction::LockScreen, Handler::Shell(ShellVerb::Lock)),
    (
        StandardAction::PreviousApp,
        Handler::Shell(ShellVerb::SwitcherPrev),
    ),
    (
        StandardAction::ScreenshotScreen,
        Handler::Shell(ShellVerb::ScreenshotScreen),
    ),
    (
        StandardAction::ScreenshotSelection,
        Handler::Shell(ShellVerb::ScreenshotArea),
    ),
    (
        StandardAction::ScreenshotTools,
        Handler::Shell(ShellVerb::ScreenshotArea),
    ),
    (
        StandardAction::LogOut,
        Handler::Shell(ShellVerb::PowerMenuOpen),
    ),
    (StandardAction::TileLeft, Handler::Compositor),
    (StandardAction::TileRight, Handler::Compositor),
    (StandardAction::TileTop, Handler::Compositor),
    (StandardAction::TileBottom, Handler::Compositor),
    (StandardAction::TileTopLeft, Handler::Compositor),
    (StandardAction::TileTopRight, Handler::Compositor),
    (StandardAction::TileBottomLeft, Handler::Compositor),
    (StandardAction::TileBottomRight, Handler::Compositor),
    (StandardAction::Fill, Handler::Compositor),
    (StandardAction::Center, Handler::Compositor),
    (StandardAction::ReturnToPreviousSize, Handler::Compositor),
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
