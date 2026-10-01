//! The one error the harness returns: a message that says what was expected and what was seen.

use std::fmt;

/// Why a harness step failed. The text is the whole point: a CI log of it must be enough to
/// see what went wrong without rerunning.
#[derive(Clone, PartialEq, Eq)]
pub struct Failure(String);

impl Failure {
    /// A failure carrying `message`.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The same failure with `context` in front, for the step that was running.
    pub fn during(self, context: &str) -> Self {
        Self(format!("{context}: {}", self.0))
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `Debug` prints the message as is, so `unwrap` on a harness result reads as a sentence.
impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failure {}

/// An I/O error as a failure that names what was being done.
pub(crate) fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Failure {
    let context = context.into();
    move |e| Failure(format!("{context}: {e}"))
}

/// An errno as a failure that names what was being done.
pub(crate) fn errno(context: impl Into<String>) -> impl FnOnce(rustix::io::Errno) -> Failure {
    let context = context.into();
    move |e| Failure(format!("{context}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_goes_in_front_and_debug_is_the_message() {
        let f = Failure::new("no node").during("opening output");
        assert_eq!(f.to_string(), "opening output: no node");
        assert_eq!(format!("{f:?}"), "opening output: no node");
    }
}
