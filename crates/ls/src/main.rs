use std::ffi::OsString;
use std::fs::{self, FileType};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Dir,
    File,
    Symlink,
    Fifo,
    Socket,
    Block,
    Char,
    Unknown,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Dir => "dir",
            Kind::File => "file",
            Kind::Symlink => "link",
            Kind::Fifo => "fifo",
            Kind::Socket => "socket",
            Kind::Block => "block",
            Kind::Char => "char",
            Kind::Unknown => "?",
        }
    }
}

impl From<FileType> for Kind {
    fn from(ft: FileType) -> Self {
        if ft.is_dir() {
            Kind::Dir
        } else if ft.is_file() {
            Kind::File
        } else if ft.is_symlink() {
            Kind::Symlink
        } else if ft.is_fifo() {
            Kind::Fifo
        } else if ft.is_socket() {
            Kind::Socket
        } else if ft.is_block_device() {
            Kind::Block
        } else if ft.is_char_device() {
            Kind::Char
        } else {
            Kind::Unknown
        }
    }
}

struct Entry {
    name: OsString,
    kind: Kind,
}

fn read_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for dirent in fs::read_dir(path)? {
        let dirent = dirent?;
        let name = dirent.file_name();
        if name.as_bytes().starts_with(b".") {
            continue;
        }
        let kind = dirent.file_type().map_or(Kind::Unknown, Kind::from);
        entries.push(Entry { name, kind });
    }
    sort_entries(&mut entries);
    Ok(entries)
}

fn sort_entries(entries: &mut [Entry]) {
    // false < true, so directories come first; raw bytes break ties between names that only differ
    // in case
    entries.sort_by_cached_key(|e| {
        (
            e.kind != Kind::Dir,
            e.name.to_string_lossy().to_lowercase(),
            e.name.clone(),
        )
    });
}

fn main() {
    let entries = read_entries(Path::new(".")).unwrap();
    for entry in &entries {
        println!("{}\t{}", entry.name.to_string_lossy(), entry.kind.label());
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn entry(name: &str, kind: Kind) -> Entry {
        Entry {
            name: name.into(),
            kind,
        }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.to_str().unwrap()).collect()
    }

    #[test]
    fn dirs_first_then_name_ignoring_case() {
        let mut entries = vec![
            entry("zeta.txt", Kind::File),
            entry("src", Kind::Dir),
            entry("README.md", Kind::File),
            entry("Docs", Kind::Dir),
            entry("cargo.lock", Kind::File),
            entry("link", Kind::Symlink),
        ];
        sort_entries(&mut entries);
        assert_eq!(
            names(&entries),
            ["Docs", "src", "cargo.lock", "link", "README.md", "zeta.txt"]
        );
    }

    #[test]
    fn same_name_different_case_is_stable() {
        let mut entries = vec![entry("b", Kind::File), entry("B", Kind::File)];
        sort_entries(&mut entries);
        assert_eq!(names(&entries), ["B", "b"]);
    }
}
