//! `[[device]]` entries: which layout a keyboard has when its vendor id cannot say. A vendor
//! says Apple for Apple keyboards; a keyboard with a Mac/Windows switch (Keychron, ...) looks
//! the same to the host either way, so the user names it.

use keycap_protocol::Warning;
use toml::{Table, Value};

use super::layer::ConfigLayer;
use super::lenient::{dotted, warning};
use crate::layout::Layout;

/// How a `[[device]]` entry picks its keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DeviceMatch {
    /// `"3434:0a53"`: USB or Bluetooth vendor and product id, in hex.
    Id { vendor: u16, product: u16 },
    /// Any other text: the device's exact name.
    Name(String),
}

impl DeviceMatch {
    fn parse(text: &str) -> DeviceMatch {
        let ids = text.split_once(':').and_then(|(v, p)| {
            Some((
                u16::from_str_radix(v.trim(), 16).ok()?,
                u16::from_str_radix(p.trim(), 16).ok()?,
            ))
        });
        match ids {
            Some((vendor, product)) => DeviceMatch::Id { vendor, product },
            None => DeviceMatch::Name(text.to_owned()),
        }
    }

    fn matches(&self, name: &str, vendor: u16, product: u16) -> bool {
        match self {
            DeviceMatch::Id {
                vendor: v,
                product: p,
            } => (*v, *p) == (vendor, product),
            DeviceMatch::Name(n) => n == name,
        }
    }
}

/// One entry: the keyboard it picks and the layout it gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceEntry {
    matcher: DeviceMatch,
    layout: Layout,
}

/// The layouts the config assigns, from the layers folded together.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutOverrides(Vec<DeviceEntry>);

impl LayoutOverrides {
    /// The layout configured for the keyboard, if any entry picks it. Later layers and later
    /// entries win.
    pub fn layout_of(&self, name: &str, vendor: u16, product: u16) -> Option<Layout> {
        self.0
            .iter()
            .rev()
            .find(|e| e.matcher.matches(name, vendor, product))
            .map(|e| e.layout)
    }
}

/// The overrides of `layers`, earliest first.
pub fn layout_overrides(layers: &[ConfigLayer]) -> LayoutOverrides {
    LayoutOverrides(layers.iter().flat_map(|l| l.devices.clone()).collect())
}

/// Reads the `device` array of a file's root table.
pub(super) fn read_devices(root: &Table, warn: &mut Vec<Warning>) -> Vec<DeviceEntry> {
    match root.get("device") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .filter_map(|(i, v)| read_entry(v, &format!("device[{i}]"), warn))
            .collect(),
        Some(_) => {
            warn.push(warning("device", "is not a list of tables ([[device]])"));
            Vec::new()
        }
    }
}

fn read_entry(v: &Value, path: &str, warn: &mut Vec<Warning>) -> Option<DeviceEntry> {
    let Some(table) = v.as_table() else {
        warn.push(warning(path, "is not a table"));
        return None;
    };
    let text = |key: &str, warn: &mut Vec<Warning>| match table.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(_) => {
            warn.push(warning(&dotted(path, key), "is not a non-empty string"));
            None
        }
        None => {
            warn.push(warning(&dotted(path, key), "is missing"));
            None
        }
    };
    let matcher = text("match", warn).map(|s| DeviceMatch::parse(&s));
    let layout = text("layout", warn).and_then(|s| {
        let found = Layout::named(&s);
        if found.is_none() {
            warn.push(warning(
                &dotted(path, "layout"),
                format!("{s:?} is not a layout (apple, pc)"),
            ));
        }
        found
    });
    Some(DeviceEntry {
        matcher: matcher?,
        layout: layout?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigSource;

    fn overrides(text: &str) -> (LayoutOverrides, Vec<Warning>) {
        let (layer, warnings) = ConfigLayer::parse(text, ConfigSource::System);
        (layout_overrides(&[layer]), warnings)
    }

    #[test]
    fn an_entry_picks_by_vendor_and_product_or_by_exact_name() {
        let (o, w) = overrides(
            "[[device]]\nmatch = \"3434:0A53\"\nlayout = \"apple\"\n\
             [[device]]\nmatch = \"My Board\"\nlayout = \"pc\"\n",
        );
        assert_eq!(w, []);
        assert_eq!(
            o.layout_of("Keychron K5 Max Keyboard", 0x3434, 0x0a53),
            Some(Layout::Apple)
        );
        assert_eq!(o.layout_of("Keychron K3", 0x3434, 0x0a54), None);
        assert_eq!(o.layout_of("My Board", 1, 2), Some(Layout::Pc));
        assert_eq!(o.layout_of("My Board 2", 1, 2), None);
    }

    #[test]
    fn the_later_entry_and_the_later_layer_win() {
        let (o, _) = overrides(
            "[[device]]\nmatch = \"1:2\"\nlayout = \"apple\"\n[[device]]\nmatch = \"1:2\"\nlayout = \"pc\"\n",
        );
        assert_eq!(o.layout_of("x", 1, 2), Some(Layout::Pc));
        let (a, _) = ConfigLayer::parse(
            "[[device]]\nmatch = \"1:2\"\nlayout = \"apple\"\n",
            ConfigSource::System,
        );
        let (b, _) = ConfigLayer::parse(
            "[[device]]\nmatch = \"1:2\"\nlayout = \"pc\"\n",
            ConfigSource::User,
        );
        assert_eq!(
            layout_overrides(&[a, b]).layout_of("x", 1, 2),
            Some(Layout::Pc)
        );
    }

    #[test]
    fn a_bad_entry_costs_itself_with_a_warning() {
        let (o, w) = overrides(
            "[[device]]\nmatch = \"1:2\"\nlayout = \"dvorak\"\n[[device]]\nlayout = \"pc\"\n",
        );
        assert_eq!(o, LayoutOverrides::default());
        let keys: Vec<_> = w.iter().map(|w| w.key.as_str()).collect();
        assert_eq!(keys, ["device[0].layout", "device[1].match"]);
    }
}
