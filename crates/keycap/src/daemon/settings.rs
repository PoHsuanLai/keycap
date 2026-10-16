//! The config layers in force, and the profiles they resolve to.

use crate::{
    ConfigLayer, ConfigSource, LayoutOverrides, Oversize, Profiles, builtin, layout_overrides,
    resolve, within_limits,
};
use keycap_protocol::Warning;

/// The four layers, earliest first: compiled-in defaults, the vendor file (what the desktop
/// ships), the system file (the administrator's), the seat owner's settings. Each starts empty
/// except the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layers {
    builtin: ConfigLayer,
    vendor: ConfigLayer,
    system: ConfigLayer,
    user: ConfigLayer,
}

impl Layers {
    /// Only the compiled-in defaults.
    pub(crate) fn builtin_only() -> Self {
        Self {
            builtin: builtin().0,
            vendor: ConfigLayer::default(),
            system: ConfigLayer::default(),
            user: ConfigLayer::default(),
        }
    }

    /// These layers with the vendor file's new contents.
    pub(crate) fn with_vendor(&self, vendor: ConfigLayer) -> Self {
        Self {
            vendor,
            ..self.clone()
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

    /// These layers if they fold into a configuration within the size limits, else why not.
    /// A layer arriving from outside is taken only through this, so an oversize one leaves the
    /// settings in force as they were.
    pub(crate) fn accepted(self) -> Result<Self, Oversize> {
        within_limits(&self.stack())?;
        Ok(self)
    }

    /// The layers earliest first, as they fold.
    fn stack(&self) -> [ConfigLayer; 4] {
        [
            self.builtin.clone(),
            self.vendor.clone(),
            self.system.clone(),
            self.user.clone(),
        ]
    }

    /// The seat owner's settings text as the socket delivers it: read, then cut down to what
    /// their own settings may say under the machine files' `[user_layer]` choice (the system
    /// file's, else the vendor file's). Refused whole
    /// (`Oversize`) when it holds too many rules; the layers in force are untouched either way.
    pub(crate) fn offered(&self, toml: &str) -> Result<(ConfigLayer, Vec<Warning>), Oversize> {
        let (layer, mut warnings) = parse_user(toml);
        let (layer, cut) = layer.confined(self.system.sequences_over(&self.vendor))?;
        warnings.extend(cut);
        Ok((layer, warnings))
    }

    /// The seat owner has gone: their settings no longer apply.
    pub(crate) fn without_user(&self) -> Self {
        self.with_user(ConfigLayer::default())
    }

    /// The keyboard layouts the layers assign.
    pub(crate) fn overrides(&self) -> LayoutOverrides {
        layout_overrides(&self.stack())
    }

    /// The profiles the layers fold into, and what the folding found wrong.
    pub(crate) fn resolve(&self) -> (Profiles, Vec<Warning>) {
        resolve(&self.stack())
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
    fn the_vendor_file_lies_over_the_built_in_mapping_and_under_the_machine_file() {
        const CAPS: &str = "capslock";
        let caps = |ctrl_or_alt: &str| {
            format!(
                "[profile.default]\nrules = [{{ when = \"{CAPS}\", then = \"{ctrl_or_alt}\" }}]\n"
            )
        };
        let stroke = |s: &str| {
            crate::RuleAction::Map(crate::Stroke::parse(s).unwrap(), crate::Hold::Restore)
        };
        let cases: [(
            &str,
            Option<String>,
            Option<String>,
            Option<crate::RuleAction>,
        ); 4] = [
            ("neither file", None, None, None),
            (
                "the vendor file alone",
                Some(caps("ctrl+space")),
                None,
                Some(stroke("ctrl+space")),
            ),
            (
                "the machine file alone",
                None,
                Some(caps("alt+space")),
                Some(stroke("alt+space")),
            ),
            (
                "the machine file over the vendor file",
                Some(caps("ctrl+space")),
                Some(caps("alt+space")),
                Some(stroke("alt+space")),
            ),
        ];
        for (case, vendor, system, want) in cases {
            let mut layers = Layers::builtin_only();
            if let Some(text) = vendor {
                layers = layers.with_vendor(layer(&text, ConfigSource::Vendor));
            }
            if let Some(text) = system {
                layers = layers.with_system(layer(&text, ConfigSource::System));
            }
            let (profiles, _) = layers.resolve();
            let trigger = crate::Trigger::parse(CAPS).unwrap();
            assert_eq!(
                profiles.default_profile().rules.get(&trigger),
                want.as_ref(),
                "{case}"
            );
        }
    }

    #[test]
    fn a_vendor_rule_on_a_bare_key_stands_where_the_same_rule_from_the_person_is_cut() {
        const BODY: &str =
            "[profile.default]\nrules = [{ when = \"capslock\", then = \"ctrl+space\" }]\n";
        let layers = Layers::builtin_only().with_vendor(layer(BODY, ConfigSource::Vendor));
        let trigger = crate::Trigger::parse("capslock").unwrap();
        let (profiles, _) = layers.resolve();
        assert!(profiles.default_profile().rules.contains_key(&trigger));
        let (offered, warnings) = layers.offered(BODY).unwrap();
        assert_eq!(warnings.len(), 1, "the person's copy is reported");
        let (profiles, _) = Layers::builtin_only().with_user(offered).resolve();
        assert!(!profiles.default_profile().rules.contains_key(&trigger));
    }

    #[test]
    fn the_vendor_file_may_let_lists_through_until_the_machine_file_says_otherwise() {
        const LIST: &str = "[profile.default]\nrules = [\n\
            { when = \"control+option+j\", then = [\"a\", \"b\"] },\n]\n";
        let allow = layer(
            "[user_layer]\nsequences = \"allow\"\n",
            ConfigSource::Vendor,
        );
        let refuse = layer(
            "[user_layer]\nsequences = \"refuse\"\n",
            ConfigSource::System,
        );
        let vendor_only = Layers::builtin_only().with_vendor(allow);
        let kept = |layers: &Layers| layers.offered(LIST).unwrap().1.len();
        assert_eq!(kept(&vendor_only), 0, "the vendor's choice stands");
        assert_eq!(
            kept(&vendor_only.with_system(refuse)),
            1,
            "the machine file's wins"
        );
    }

    #[test]
    fn an_oversize_layer_is_not_accepted_and_the_layers_in_force_are_untouched() {
        let in_force = Layers::builtin_only();
        let huge: String = (0..=crate::MAX_PROFILES)
            .map(|i| format!("[profile.p{i}]\n"))
            .collect();
        let (layer, warnings) = parse_user(&huge);
        assert_eq!(warnings, []);
        let refused = in_force.with_user(layer).accepted();
        assert!(matches!(refused, Err(Oversize::Profiles { .. })));
        assert_eq!(in_force, Layers::builtin_only());
        let small = parse_user("[profile.work]\nmodmap = { command = \"alt\" }\n").0;
        assert!(in_force.with_user(small).accepted().is_ok());
    }

    #[test]
    fn what_the_socket_delivers_is_cut_down_by_the_system_files_choice_about_lists() {
        const BODY: &str = "[profile.default]\nrules = [\n\
            { when = \"enter\", then = \"a\" },\n\
            { when = \"control+option+j\", then = [\"a\", \"b\"] },\n\
            { when = \"control+option+k\", then = \"k\" },\n]\n";
        let in_force = |layers: &Layers| {
            let (layer, warnings) = layers.offered(BODY).unwrap();
            let (profiles, _) = layers.with_user(layer).resolve();
            let has = |when: &str| {
                let trigger = crate::Trigger::parse(when).unwrap();
                profiles.default_profile().rules.contains_key(&trigger)
            };
            (
                warnings.len(),
                has("enter"),
                has("control+option+j"),
                has("control+option+k"),
            )
        };
        let refusing = Layers::builtin_only();
        assert_eq!(in_force(&refusing), (2, false, false, true), "the default");
        let allowing = refusing.with_system(layer(
            "[user_layer]\nsequences = \"allow\"\n",
            ConfigSource::System,
        ));
        assert_eq!(
            in_force(&allowing),
            (1, false, true, true),
            "the list is let through, the bare key is not"
        );
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
