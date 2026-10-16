//! A text file read safely: it must be a regular file, it is opened without blocking, and only
//! a bounded amount is read. The files are the person's own, in a directory they can write, so
//! a named pipe put in the file's place must not hang a program's start-up and a huge file must
//! not fill its memory.

use rustix::fs::OFlags;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
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
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(OFlags::NONBLOCK.bits() as i32)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return FileText::Missing,
        Err(e) => return FileText::Unusable(e.to_string()),
    };
    match file.metadata() {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return FileText::Unusable("it is not a regular file".to_owned()),
        Err(e) => return FileText::Unusable(e.to_string()),
    }
    let mut text = String::new();
    match file
        .take(max_bytes.saturating_add(1))
        .read_to_string(&mut text)
    {
        Ok(n) if u64::try_from(n).is_ok_and(|n| n > max_bytes) => {
            FileText::Unusable(format!("it is larger than {} KiB", max_bytes / 1024))
        }
        Ok(_) => FileText::Text(text),
        Err(e) if e.kind() == ErrorKind::InvalidData => {
            FileText::Unusable("it is not plain text".to_owned())
        }
        Err(e) => FileText::Unusable(e.to_string()),
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
        assert!(
            matches!(read_text(&path, LIMIT), FileText::Unusable(w) if w.contains("plain text"))
        );
    }

    #[test]
    fn a_pipe_or_a_directory_in_its_place_is_unusable_and_does_not_hang() {
        let dir = Scratch::new("bounded-pipe");
        let pipe = dir.path().join("pipe");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            pipe.as_path(),
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        assert!(matches!(read_text(&pipe, LIMIT), FileText::Unusable(w) if w.contains("regular")));
        let folder = dir.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        assert!(matches!(read_text(&folder, LIMIT), FileText::Unusable(_)));
    }

    #[test]
    fn a_symbolic_link_to_a_regular_file_is_followed() {
        let dir = Scratch::new("bounded-link");
        let target = dir.path().join("real");
        std::fs::write(&target, "undo = none\n").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(
            read_text(&link, LIMIT),
            FileText::Text("undo = none\n".into())
        );
    }
}
