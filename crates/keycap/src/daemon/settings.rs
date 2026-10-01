//! The config layers in force, and the profiles they resolve to.

use crate::{ConfigLayer, ConfigSource, Profiles, builtin, resolve};
use keycap_protocol::Warning;

/// The three layers, earliest first: compiled-in defaults, the system file, the seat owner's
/// settings. Each starts empty except the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layers {
    builtin: ConfigLayer,
    system: ConfigLayer,
    user: ConfigLayer,
}

impl Layers {
    /// Only the compiled-in defaults.
    pub(crate) fn builtin_only() -> Self {
        Self {
            builtin: builtin().0,
            system: ConfigLayer::default(),
            user: ConfigLayer::default(),
        }
    }

    /// These layers with the system file's new contents.
    pub(crate) fn with_system(&self, system: ConfigLayer) -> Self {
        Self {
            system,
            ..self.clone()
        }
    }

    /// These layers with the seat owner's new settings.
    pub(crate) fn with_user(&self, user: ConfigLayer) -> Self {
        Self {
            user,
            ..self.clone()
        }
    }

    /// The seat owner has gone: their settings no longer apply.
    pub(crate) fn without_user(&self) -> Self {
        self.with_user(ConfigLayer::default())
    }

    /// The profiles the layers fold into, and what the folding found wrong.
    pub(crate) fn resolve(&self) -> (Profiles, Vec<Warning>) {
        resolve(&[self.builtin.clone(), self.system.clone(), self.user.clone()])
    }
}

/// The seat owner's settings text as a layer, with the problems found reading it.
pub(crate) fn parse_user(toml: &str) -> (ConfigLayer, Vec<Warning>) {
    ConfigLayer::parse(toml, ConfigSource::User)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keycap_protocol::ProfileName;

    fn layer(text: &str, source: ConfigSource) -> ConfigLayer {
        let (layer, warnings) = ConfigLayer::parse(text, source);
        assert_eq!(warnings, []);
        layer
    }

    fn name(s: &str) -> ProfileName {
        ProfileName::parse(s).unwrap()
    }

    const TERMINAL: &str = "[profile.work]\nmodmap = { command = \"alt\" }\n";

    #[test]
    fn later_layers_override_and_clearing_the_user_restores_the_system() {
        let system = layer(TERMINAL, ConfigSource::System);
        let user = layer(
            "[profile.work]\nmodmap = { command = \"super\" }\n",
            ConfigSource::User,
        );
        let command_of = |layers: &Layers| {
            let (profiles, _) = layers.resolve();
            profiles.resolve(&name("work")).modmap.command
        };
        let base = Layers::builtin_only();
        let with_system = base.with_system(system);
        let with_user = with_system.with_user(user);
        assert_eq!(command_of(&base), crate::OutMod::Ctrl, "built-in modmap");
        assert_eq!(
            command_of(&with_system),
            crate::OutMod::Alt,
            "system over built-in"
        );
        assert_eq!(
            command_of(&with_user),
            crate::OutMod::Super,
            "user over system"
        );
        assert_eq!(command_of(&with_user.without_user()), crate::OutMod::Alt);
    }

    #[test]
    fn a_user_layer_with_a_bad_value_is_reported_and_costs_only_that_value() {
        let (layer, warnings) = parse_user("[profile.work]\nmodmap = { command = \"nope\" }\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].problem.contains("user config"), "{warnings:?}");
        let (profiles, _) = Layers::builtin_only().with_user(layer).resolve();
        assert!(profiles.defines(&ProfileName::default_profile()));
    }
}
