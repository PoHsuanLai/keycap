//! The compiled-in Mac mapping.

use keycap_protocol::Warning;

use super::layer::{ConfigLayer, ConfigSource};

/// The text of `defaults.toml`, exactly as shipped.
pub const DEFAULTS_TOML: &str = include_str!("defaults.toml");

/// The built-in layer, every default mapping. The warnings are empty for the shipped file; a
/// test holds that.
pub fn builtin() -> (ConfigLayer, Vec<Warning>) {
    ConfigLayer::parse(DEFAULTS_TOML, ConfigSource::Builtin)
}
