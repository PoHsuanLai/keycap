//! Reading the keymap file from the config directory.

use keycap_client::{FileText, read_text};
use std::path::Path;

/// The file name, in the config directory next to `keycap.toml`.
pub const FILE_NAME: &str = keycap_protocol::KEYMAP_FILE;
/// The largest keymap file read: an overrides file is a short list, and the served copy must
/// fit one socket packet.
pub const MAX_BYTES: u64 = keycap_protocol::KEYMAP_MAX_BYTES;

/// Reads `FILE_NAME` in `dir`.
pub fn read(dir: &Path) -> FileText {
    read_text(&dir.join(FILE_NAME), MAX_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::scratch::Scratch;

    #[test]
    fn a_missing_file_is_missing_and_a_file_is_its_text() {
        let dir = Scratch::new("keymap-file");
        assert_eq!(read(dir.path()), FileText::Missing);
        std::fs::write(dir.path().join(FILE_NAME), "copy = Primary+C\n").unwrap();
        assert_eq!(
            read(dir.path()),
            FileText::Text("copy = Primary+C\n".into())
        );
    }

    #[test]
    fn a_file_that_is_too_big_or_not_text_is_unusable() {
        let dir = Scratch::new("keymap-file-bad");
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, vec![b'#'; usize::try_from(MAX_BYTES).unwrap() + 1]).unwrap();
        assert!(matches!(read(dir.path()), FileText::Unusable(why) if why.contains("larger")));
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(read(dir.path()), FileText::Unusable(why) if why.contains("plain text")));
        std::fs::write(&path, vec![b'#'; usize::try_from(MAX_BYTES).unwrap()]).unwrap();
        assert!(
            matches!(read(dir.path()), FileText::Text(_)),
            "exactly the limit is fine"
        );
    }

    #[test]
    fn a_directory_in_its_place_is_unusable() {
        let dir = Scratch::new("keymap-file-dir");
        std::fs::create_dir(dir.path().join(FILE_NAME)).unwrap();
        assert!(matches!(read(dir.path()), FileText::Unusable(_)));
    }
}
