//! The compiled-in Mac mapping.

use keycap_protocol::Warning;

use super::layer::{Layer, Source};

const DEFAULTS: &str = include_str!("defaults.toml");

/// The built-in layer, every default mapping. The warnings are empty for the shipped file; a
/// test holds that.
pub fn builtin() -> (Layer, Vec<Warning>) {
    Layer::parse(DEFAULTS, Source::Builtin)
}
