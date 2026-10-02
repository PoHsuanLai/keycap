//! A node's capability masks as sysfs prints them.

/// Bits per word of a sysfs capability mask (`BITS_PER_LONG` on a 64-bit kernel).
const WORD_BITS: u32 = 64;

/// A sysfs capability mask (`device/capabilities/{ev,key,rel}`): hex words, most significant
/// first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Bits(Vec<u64>);

impl Bits {
    /// Parses the mask's text; an unreadable word counts as empty.
    pub(crate) fn parse(text: &str) -> Self {
        Self(
            text.split_whitespace()
                .rev()
                .map(|w| u64::from_str_radix(w, 16).unwrap_or(0))
                .collect(),
        )
    }

    /// The mask an `EVIOCGBIT` or `EVIOCGPROP` ioctl returned: little-endian 64-bit words,
    /// least significant first, as a 64-bit kernel writes them.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        Self(
            bytes
                .chunks(8)
                .map(|chunk| {
                    let mut word = [0u8; 8];
                    word[..chunk.len()].copy_from_slice(chunk);
                    u64::from_le_bytes(word)
                })
                .collect(),
        )
    }

    /// Whether bit `n` is set.
    pub(crate) fn has(&self, n: u32) -> bool {
        let word = usize::try_from(n / WORD_BITS).unwrap_or(usize::MAX);
        self.0
            .get(word)
            .is_some_and(|w| w >> (n % WORD_BITS) & 1 == 1)
    }
}

/// The event types, keys, relative and absolute axes a node can send.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Caps {
    pub ev: Bits,
    pub key: Bits,
    pub rel: Bits,
    pub abs: Bits,
}
