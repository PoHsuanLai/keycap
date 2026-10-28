//! The config name of each variant of a closed set.

/// Gives a fieldless enum `ALL` (declaration order), `name` (the word the config uses) and
/// `named` (the variant a word stands for), from one list of variant and word pairs.
macro_rules! names {
    ($ty:ident { $($variant:ident => $word:literal),+ $(,)? }) => {
        impl $ty {
            /// Every variant, in declaration order.
            pub const ALL: [$ty; [$($word),+].len()] = [$($ty::$variant),+];

            /// The variant the config calls `name`.
            pub fn named(name: &str) -> Option<$ty> {
                Self::ALL.into_iter().find(|v| v.name() == name)
            }

            /// The word the config uses for this variant.
            pub const fn name(self) -> &'static str {
                match self {
                    $($ty::$variant => $word),+
                }
            }
        }
    };
}
