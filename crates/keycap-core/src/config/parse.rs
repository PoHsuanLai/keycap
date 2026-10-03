//! `ConfigLayer::parse`: TOML text to a `ConfigLayer`, key by key.

use std::collections::BTreeMap;

use keycap_protocol::{ProfileName, Warning};
use toml::{Table, Value};

use super::layer::{ConfigLayer, ConfigSource, PartialModMap, ProfileLayer};
use super::lenient::{Setting, dotted, field, section, warning};
use crate::profile::{Hold, RuleAction, Stroke, Trigger};

impl ConfigLayer {
    /// Reads one file's text. Unknown keys are ignored; a bad value costs only its own key (or
    /// its own rule), reported under its dotted path such as `profile.terminal.rules[2].when`;
    /// text that is not TOML is an empty layer and one warning. Where one file gives a trigger
    /// twice, the later rule wins.
    pub fn parse(text: &str, source: ConfigSource) -> (ConfigLayer, Vec<Warning>) {
        let (layer, warnings) = match text.parse::<Table>() {
            Ok(root) => read_layer(&root),
            Err(e) => (
                ConfigLayer::default(),
                vec![warning("file", format!("is not valid TOML: {e}"))],
            ),
        };
        let attributed = warnings
            .into_iter()
            .map(|w| Warning {
                problem: format!("{source}: {}", w.problem),
                ..w
            })
            .collect();
        (layer, attributed)
    }
}

fn read_layer(root: &Table) -> (ConfigLayer, Vec<Warning>) {
    let mut warn = Vec::new();
    let profiles = section(root, "", "profile", &mut warn)
        .iter()
        .filter_map(|(name, v)| read_profile(name, v, &mut warn))
        .collect();
    let devices = super::devices::read_devices(root, &mut warn);
    (ConfigLayer { profiles, devices }, warn)
}

fn read_profile(
    name: &str,
    v: &Value,
    warn: &mut Vec<Warning>,
) -> Option<(ProfileName, ProfileLayer)> {
    let path = dotted("profile", name);
    let profile_name = ProfileName::parse(name)
        .map_err(|e| warn.push(warning(&path, e.to_string())))
        .ok()?;
    let Some(table) = v.as_table() else {
        warn.push(warning(&path, "is not a table"));
        return None;
    };
    let layer = ProfileLayer {
        inherits: field(table, &path, "inherits", warn),
        modmap: read_modmap(
            &section(table, &path, "modmap", warn),
            &dotted(&path, "modmap"),
            warn,
        ),
        rules: read_rules(table, &path, warn),
    };
    Some((profile_name, layer))
}

fn read_modmap(table: &Table, path: &str, warn: &mut Vec<Warning>) -> PartialModMap {
    PartialModMap {
        command: field(table, path, "command", warn),
        option: field(table, path, "option", warn),
        control: field(table, path, "control", warn),
    }
}

fn read_rules(table: &Table, path: &str, warn: &mut Vec<Warning>) -> BTreeMap<Trigger, RuleAction> {
    let key = dotted(path, "rules");
    match table.get("rules") {
        None => BTreeMap::new(),
        Some(Value::Array(rules)) => rules
            .iter()
            .enumerate()
            .filter_map(|(i, v)| read_rule(v, &format!("{key}[{i}]"), warn))
            .collect(),
        Some(_) => {
            warn.push(warning(&key, "is not a list of rules"));
            BTreeMap::new()
        }
    }
}

/// One rule. A bad `when` or `then` costs the rule; a bad `hold` costs only the hold.
fn read_rule(v: &Value, path: &str, warn: &mut Vec<Warning>) -> Option<(Trigger, RuleAction)> {
    let Some(table) = v.as_table() else {
        warn.push(warning(path, "is not a table"));
        return None;
    };
    let when = required(table, path, "when", warn);
    let hold = field::<Hold>(table, path, "hold", warn);
    let then = match table.get("then") {
        Some(v) => read_then(v, &dotted(path, "then"), hold, warn),
        None => {
            warn.push(warning(&dotted(path, "then"), "is missing"));
            None
        }
    };
    if hold.is_some() && !matches!(then, Some(RuleAction::Map(..)) | None) {
        warn.push(warning(
            &dotted(path, "hold"),
            "applies to a single stroke, not a list; ignored",
        ));
    }
    Some((when?, then?))
}

fn required(table: &Table, path: &str, key: &str, warn: &mut Vec<Warning>) -> Option<Trigger> {
    match table.get(key) {
        Some(v) => Trigger::read(v, &dotted(path, key), warn),
        None => {
            warn.push(warning(&dotted(path, key), "is missing"));
            None
        }
    }
}

/// A string is one `Map` stroke; a list is a `Sequence`, and the empty list is `Swallow`.
/// `hold` belongs to `Map` alone, and the caller reports it on a list.
fn read_then(
    v: &Value,
    path: &str,
    hold: Option<Hold>,
    warn: &mut Vec<Warning>,
) -> Option<RuleAction> {
    match v {
        Value::Array(items) => {
            let strokes: Vec<Option<Stroke>> = items
                .iter()
                .enumerate()
                .map(|(i, item)| Stroke::read(item, &format!("{path}[{i}]"), warn))
                .collect();
            strokes
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .map(RuleAction::sequence)
        }
        single => Stroke::read(single, path, warn)
            .map(|stroke| RuleAction::Map(stroke, hold.unwrap_or_default())),
    }
}
