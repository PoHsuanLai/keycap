//! Reading one key leniently: a bad value costs that key only, as a `Warning`.

use keycap_protocol::{ProfileName, Warning};
use toml::{Table, Value};

use super::confine::Sequences;
use crate::profile::{Hold, Stroke, Trigger};
use crate::role::OutMod;

/// A value that can be read from one TOML value.
pub(super) trait Setting: Sized {
    /// The value, or `None` with a warning pushed.
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self>;
}

pub(super) fn warning(key: &str, problem: impl Into<String>) -> Warning {
    Warning {
        key: key.to_owned(),
        problem: problem.into(),
    }
}

/// The dotted name of `key` inside the table at `path`.
pub(super) fn dotted(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Reads `key` of `table`; `None` when absent, or unusable with a warning.
pub(super) fn field<T: Setting>(
    table: &Table,
    path: &str,
    key: &str,
    warn: &mut Vec<Warning>,
) -> Option<T> {
    T::read(table.get(key)?, &dotted(path, key), warn)
}

/// The sub-table `key` of `table`, or an empty one (with a warning if `key` is not a table).
pub(super) fn section(table: &Table, path: &str, key: &str, warn: &mut Vec<Warning>) -> Table {
    match table.get(key) {
        None => Table::new(),
        Some(Value::Table(t)) => t.clone(),
        Some(_) => {
            warn.push(warning(&dotted(path, key), "is not a table"));
            Table::new()
        }
    }
}

/// A string value run through `parse`; the failure's text becomes the warning.
fn text<T>(
    v: &Value,
    key: &str,
    warn: &mut Vec<Warning>,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Option<T> {
    let parsed = v
        .as_str()
        .ok_or_else(|| "is not a string".to_owned())
        .and_then(parse);
    parsed
        .map_err(|problem| warn.push(warning(key, problem)))
        .ok()
}

impl Setting for OutMod {
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self> {
        text(v, key, warn, |s| {
            OutMod::named(s)
                .ok_or_else(|| format!("{s:?} is not an output modifier (ctrl, alt, super, shift)"))
        })
    }
}

impl Setting for Hold {
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self> {
        text(v, key, warn, |s| {
            Hold::named(s).ok_or_else(|| format!("{s:?} is not a hold (restore, while_held)"))
        })
    }
}

impl Setting for Sequences {
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self> {
        text(v, key, warn, |s| {
            Sequences::named(s).ok_or_else(|| format!("{s:?} is not a choice (refuse, allow)"))
        })
    }
}

impl Setting for Trigger {
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self> {
        text(v, key, warn, |s| {
            Trigger::parse(s).map_err(|e| e.to_string())
        })
    }
}

impl Setting for Stroke {
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self> {
        text(v, key, warn, |s| {
            Stroke::parse(s).map_err(|e| e.to_string())
        })
    }
}

impl Setting for ProfileName {
    fn read(v: &Value, key: &str, warn: &mut Vec<Warning>) -> Option<Self> {
        text(v, key, warn, |s| {
            ProfileName::parse(s).map_err(|e| e.to_string())
        })
    }
}
