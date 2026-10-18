//! A text file read safely, through chordkit's bounded reader (regular files only, opened without
//! blocking, at most a named number of bytes, UTF-8). The files are the person's own, in a
//! directory they can write, so a named pipe put in the file's place must not hang a program's
//! start-up and a huge file must not fill its memory.

use chordkit_file::{ByteLimit, ReadError};
use std::path::Path;

/// What reading a text file came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileText {
    /// There is no such file.
    Missing,
    /// The file's text.
    Text(String),
    /// The file is there but cannot be used, with the reason in plain words.
    Unusable(String),
}

/// Reads the text of the regular file at `path`, at most `max_bytes` of it. A symbolic link to a
/// regular file is followed (a dotfile manager's); anything that is not a regular file, a file
/// over the limit and one that is not UTF-8 text are `Unusable`.
pub fn read_text(path: &Path, max_bytes: u64) -> FileText {
    match chordkit_file::read_text(path, ByteLimit::new(max_bytes)) {
        Ok(text) => FileText::Text(text),
        Err(ReadError::Missing) => FileText::Missing,
        Err(why) => FileText::Unusable(why.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Scratch;

    const LIMIT: u64 = 2048;

    #[test]
    fn missing_text_too_big_and_not_text() {
        let dir = Scratch::new("bounded");
        let path = dir.path().join("f");
        assert_eq!(read_text(&path, LIMIT), FileText::Missing);
        std::fs::write(&path, "copy = Primary+C\n").unwrap();
        assert_eq!(
            read_text(&path, LIMIT),
            FileText::Text("copy = Primary+C\n".into())
        );
        std::fs::write(&path, vec![b'#'; LIMIT as usize]).unwrap();
        assert!(
            matches!(read_text(&path, LIMIT), FileText::Text(_)),
            "the limit itself"
        );
        std::fs::write(&path, vec![b'#'; LIMIT as usize + 1]).unwrap();
        assert!(matches!(read_text(&path, LIMIT), FileText::Unusable(w) if w.contains("larger")));
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(matches!(read_text(&path, LIMIT), FileText::Unusable(w) if w.contains("text")));
    }
}
