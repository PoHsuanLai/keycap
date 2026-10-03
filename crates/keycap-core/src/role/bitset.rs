//! The one bitset shape shared by `Roles` and `OutMods`.

/// Declares a set newtype over `u8` for a fieldless enum with `ALL` and a `bit` method.
macro_rules! bitset {
    ($(#[$meta:meta])* $set:ident of $elem:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $set(u8);

        impl $set {
            /// The set with no members.
            pub const EMPTY: Self = Self(0);

            /// This set plus `member`.
            pub const fn with(self, member: $elem) -> Self {
                Self(self.0 | member.bit())
            }

            /// This set minus `member`.
            pub const fn without(self, member: $elem) -> Self {
                Self(self.0 & !member.bit())
            }

            /// Whether `member` is in the set.
            pub const fn contains(self, member: $elem) -> bool {
                self.0 & member.bit() != 0
            }

            /// Whether the set has no members.
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// Everything in `self` or `other`.
            pub const fn union(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }

            /// Everything in `self` that is not in `other`.
            pub const fn difference(self, other: Self) -> Self {
                Self(self.0 & !other.0)
            }

            /// The members, in declaration order.
            pub fn iter(self) -> impl Iterator<Item = $elem> {
                $elem::ALL.into_iter().filter(move |m| self.contains(*m))
            }
        }

        impl FromIterator<$elem> for $set {
            fn from_iter<I: IntoIterator<Item = $elem>>(members: I) -> Self {
                members.into_iter().fold(Self::EMPTY, Self::with)
            }
        }
    };
}

pub(super) use bitset;
