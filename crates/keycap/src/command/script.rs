//! The replay script: one input per line, parsed by a pure function.
//!
//! ```text
//! <ms> press|release|repeat <dev> <apple|pc> <keyname>
//! <ms> focus <profile>
//! <ms> gone <dev>
//! <ms> release-all
//! <ms> resume
//! ```
//!
//! `#` starts a comment; blank lines are skipped.

use crate::{DeviceId, Edge, Input, Key, Layout, Mono, Physical};
use keycap_protocol::{NameError, ProfileName};

/// One input of the script, with the line it came from as written (comments and extra
/// whitespace removed) so a replay can print it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Step {
    pub text: String,
    pub input: Input,
}

/// What is wrong with one line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum Problem {
    #[error("{0:?} is not a time in milliseconds")]
    Time(String),
    #[error("unknown input {0:?}")]
    Verb(String),
    #[error("`{verb}` takes {shape}")]
    Shape { verb: String, shape: &'static str },
    #[error("{0:?} is not a device number")]
    Device(String),
    #[error("{0:?} is not a layout, use apple or pc")]
    Layout(String),
    #[error("{0:?} is not a key name")]
    Key(String),
    #[error("{0}")]
    Profile(#[from] NameError),
}

/// A problem and where it is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {problem}")]
pub(super) struct ScriptError {
    pub line: usize,
    pub problem: Problem,
}

/// Every input of `text`, in order; the first bad line is the error.
pub(super) fn parse(text: &str) -> Result<Vec<Step>, ScriptError> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            parse_line(line)
                .map_err(|problem| ScriptError {
                    line: i + 1,
                    problem,
                })
                .transpose()
        })
        .collect()
}

/// One line: `None` for a blank or comment line.
fn parse_line(line: &str) -> Result<Option<Step>, Problem> {
    let content = line.split_once('#').map_or(line, |(before, _)| before);
    let words: Vec<&str> = content.split_whitespace().collect();
    let Some((&ms, rest)) = words.split_first() else {
        return Ok(None);
    };
    let at = ms
        .parse::<u64>()
        .map(Mono::from_millis)
        .map_err(|_| Problem::Time(ms.to_owned()))?;
    input_of(at, rest).map(|input| {
        Some(Step {
            text: words.join(" "),
            input,
        })
    })
}

fn input_of(at: Mono, words: &[&str]) -> Result<Input, Problem> {
    let shape = |verb: &str, shape: &'static str| Problem::Shape {
        verb: verb.to_owned(),
        shape,
    };
    match words {
        [verb @ ("press" | "release" | "repeat"), rest @ ..] => {
            let edge = match *verb {
                "press" => Edge::Press,
                "release" => Edge::Release,
                _ => Edge::Repeat,
            };
            let [device, layout, key] = rest else {
                return Err(shape(verb, "<dev> <apple|pc> <keyname>"));
            };
            Ok(Input::Key {
                device: device_of(device)?,
                layout: layout_of(layout)?,
                key: Physical(Key::named(key).ok_or_else(|| Problem::Key((*key).to_owned()))?),
                edge,
                at,
            })
        }
        ["focus", rest @ ..] => match rest {
            [profile] => Ok(Input::Focus(ProfileName::parse(profile)?)),
            _ => Err(shape("focus", "<profile>")),
        },
        ["gone", rest @ ..] => match rest {
            [device] => Ok(Input::DeviceGone(device_of(device)?)),
            _ => Err(shape("gone", "<dev>")),
        },
        ["release-all"] => Ok(Input::ReleaseAll),
        ["resume"] => Ok(Input::Resume),
        [verb @ ("release-all" | "resume"), ..] => Err(shape(verb, "no arguments")),
        [verb, ..] => Err(Problem::Verb((*verb).to_owned())),
        [] => Err(Problem::Verb(String::new())),
    }
}

fn device_of(word: &str) -> Result<DeviceId, Problem> {
    word.parse()
        .map(DeviceId)
        .map_err(|_| Problem::Device(word.to_owned()))
}

fn layout_of(word: &str) -> Result<Layout, Problem> {
    match word {
        "apple" => Ok(Layout::Apple),
        "pc" => Ok(Layout::Pc),
        other => Err(Problem::Layout(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(device: u32, layout: Layout, name: &str, edge: Edge, ms: u64) -> Input {
        Input::Key {
            device: DeviceId(device),
            layout,
            key: Physical(Key::named(name).unwrap()),
            edge,
            at: Mono::from_millis(ms),
        }
    }

    #[test]
    fn line_table() {
        let focus = |s: &str| Input::Focus(ProfileName::parse(s).unwrap());
        type Case = (&'static str, &'static str, Result<Option<Input>, Problem>);
        let cases: &[Case] = &[
            ("blank", "", Ok(None)),
            ("spaces", "   \t ", Ok(None)),
            ("comment", "# a comment", Ok(None)),
            ("comment after spaces", "   # indented", Ok(None)),
            (
                "press on apple",
                "0 press 1 apple leftmeta",
                Ok(Some(key(1, Layout::Apple, "leftmeta", Edge::Press, 0))),
            ),
            (
                "release on pc, extra spacing",
                "  25   release\t2  pc   c ",
                Ok(Some(key(2, Layout::Pc, "c", Edge::Release, 25))),
            ),
            (
                "repeat with a trailing comment",
                "40 repeat 0 apple left # held",
                Ok(Some(key(0, Layout::Apple, "left", Edge::Repeat, 40))),
            ),
            ("focus", "50 focus terminal", Ok(Some(focus("terminal")))),
            (
                "gone",
                "60 gone 3",
                Ok(Some(Input::DeviceGone(DeviceId(3)))),
            ),
            ("release all", "70 release-all", Ok(Some(Input::ReleaseAll))),
            ("resume", "80 resume", Ok(Some(Input::Resume))),
            (
                "time is not a number",
                "soon press 1 apple c",
                Err(Problem::Time("soon".into())),
            ),
            (
                "negative time",
                "-5 resume",
                Err(Problem::Time("-5".into())),
            ),
            ("time only", "10", Err(Problem::Verb(String::new()))),
            (
                "unknown verb",
                "0 tap 1 apple c",
                Err(Problem::Verb("tap".into())),
            ),
            (
                "press without a key",
                "0 press 1 apple",
                Err(Problem::Shape {
                    verb: "press".into(),
                    shape: "<dev> <apple|pc> <keyname>",
                }),
            ),
            (
                "press with too much",
                "0 press 1 apple c d",
                Err(Problem::Shape {
                    verb: "press".into(),
                    shape: "<dev> <apple|pc> <keyname>",
                }),
            ),
            (
                "bad device",
                "0 press one apple c",
                Err(Problem::Device("one".into())),
            ),
            (
                "bad layout",
                "0 press 1 dvorak c",
                Err(Problem::Layout("dvorak".into())),
            ),
            (
                "bad key",
                "0 press 1 apple KEY_C",
                Err(Problem::Key("KEY_C".into())),
            ),
            (
                "focus needs a name",
                "0 focus",
                Err(Problem::Shape {
                    verb: "focus".into(),
                    shape: "<profile>",
                }),
            ),
            (
                "bad profile name",
                "0 focus Terminal",
                Err(Problem::Profile(NameError::BadChar('T'))),
            ),
            (
                "gone needs a device",
                "0 gone",
                Err(Problem::Shape {
                    verb: "gone".into(),
                    shape: "<dev>",
                }),
            ),
            (
                "resume takes nothing",
                "0 resume now",
                Err(Problem::Shape {
                    verb: "resume".into(),
                    shape: "no arguments",
                }),
            ),
        ];
        for (name, line, want) in cases {
            let got = parse_line(line).map(|step| step.map(|s| s.input));
            assert_eq!(&got, want, "{name}");
        }
    }

    #[test]
    fn a_script_keeps_order_skips_comments_and_names_the_bad_line() {
        let text = "# cmd+c\n0 press 1 apple leftmeta\n\n10 press 1 apple c  # copy\n";
        let steps = parse(text).unwrap();
        let texts: Vec<_> = steps.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            ["0 press 1 apple leftmeta", "10 press 1 apple c"],
            "echo is the line without its comment"
        );
        let err = parse("0 resume\n# fine\n5 press 1 apple nope\n").unwrap_err();
        assert_eq!(
            err,
            ScriptError {
                line: 3,
                problem: Problem::Key("nope".into())
            }
        );
        assert_eq!(err.to_string(), "line 3: \"nope\" is not a key name");
    }
}
