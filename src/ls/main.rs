use std::ffi::{OsStr, OsString};
use std::fs::{self, FileType};
use std::io::{self, IsTerminal, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::time::SystemTime;

use unicode_width::UnicodeWidthStr;

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

struct Meta {
    len: u64,
    modified: SystemTime,
}

struct Entry {
    name: OsString,
    kind: Kind,
    // the lstat error stays with the entry so the caller can report it and the renderer can still
    // show the name and type
    meta: io::Result<Meta>,
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
        let meta = dirent.metadata().and_then(|md| {
            Ok(Meta {
                len: md.len(),
                modified: md.modified()?,
            })
        });
        entries.push(Entry { name, kind, meta });
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

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut unit = 1;
    loop {
        let div = 1u128 << (10 * unit);
        let tenths = (u128::from(bytes) * 10 + div / 2) / div;
        if tenths < 10240 || unit == UNITS.len() - 1 {
            return format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit]);
        }
        unit += 1;
    }
}

fn relative_time(then: SystemTime, now: SystemTime) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const UNITS: [(u64, &str); 6] = [
        (365 * DAY, "year"),
        (30 * DAY, "month"),
        (7 * DAY, "week"),
        (DAY, "day"),
        (HOUR, "hour"),
        (MINUTE, "minute"),
    ];

    // mtimes in the future are real; clock skew, touch -d, extracted tarballs...
    let (secs, future) = match now.duration_since(then) {
        Ok(age) => (age.as_secs(), false),
        Err(err) => (err.duration().as_secs(), true),
    };
    let Some(&(len, unit)) = UNITS.iter().find(|&&(len, _)| secs >= len) else {
        return "just now".to_string();
    };
    let n = secs / len;
    let s = if n == 1 { "" } else { "s" };
    if future {
        format!("in {n} {unit}{s}")
    } else {
        format!("{n} {unit}{s} ago")
    }
}

fn escape_name(name: &OsStr) -> String {
    let mut out = String::with_capacity(name.len());
    for chunk in name.as_bytes().utf8_chunks() {
        for c in chunk.valid().chars() {
            if c == '\\' {
                out.push_str("\\\\");
            } else if c.is_control() {
                // escape the utf-8 bytes, not the code point, so \xNN always means "this raw byte"
                // and the output stays reversible
                let mut buf = [0; 4];
                for &b in c.encode_utf8(&mut buf).as_bytes() {
                    push_byte_escape(&mut out, b);
                }
            } else {
                out.push(c);
            }
        }
        for &b in chunk.invalid() {
            push_byte_escape(&mut out, b);
        }
    }
    out
}

fn push_byte_escape(out: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push_str("\\x");
    out.push(char::from(HEX[usize::from(b >> 4)]));
    out.push(char::from(HEX[usize::from(b & 0xf)]));
}

#[derive(Clone, Copy)]
enum Layout {
    Table,
    Tsv,
}

enum Align {
    Left,
    Right,
}

const GAP: usize = 2;
const HEADER: [&str; 4] = ["name", "type", "size", "modified"];
const ALIGN: [Align; 4] = [Align::Left, Align::Left, Align::Right, Align::Left];

type Row = [String; 4];

fn row(entry: &Entry, now: SystemTime) -> Row {
    let (size, modified) = match &entry.meta {
        Ok(meta) if entry.kind == Kind::File => {
            (human_size(meta.len), relative_time(meta.modified, now))
        }
        Ok(meta) => ("-".to_string(), relative_time(meta.modified, now)),
        Err(_) => ("?".to_string(), "?".to_string()),
    };
    [
        escape_name(&entry.name),
        entry.kind.label().to_string(),
        size,
        modified,
    ]
}

fn render(
    out: &mut impl Write,
    layout: Layout,
    entries: &[Entry],
    now: SystemTime,
) -> io::Result<()> {
    let rows: Vec<Row> = entries.iter().map(|e| row(e, now)).collect();
    match layout {
        Layout::Table => table(out, &rows),
        Layout::Tsv => tsv(out, &rows),
    }
}

fn tsv(out: &mut impl Write, rows: &[Row]) -> io::Result<()> {
    for row in rows {
        writeln!(out, "{}", row.join("\t"))?;
    }
    Ok(())
}

fn table(out: &mut impl Write, rows: &[Row]) -> io::Result<()> {
    let header = HEADER.map(String::from);
    let mut widths = [0; 4];
    for row in std::iter::once(&header).chain(rows) {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.width());
        }
    }
    let rule = widths.map(|w| "─".repeat(w));
    for row in [&header, &rule].into_iter().chain(rows) {
        table_row(out, row, &widths)?;
    }
    Ok(())
}

fn table_row(out: &mut impl Write, row: &Row, widths: &[usize; 4]) -> io::Result<()> {
    let mut owed = 0;
    for ((cell, width), align) in row.iter().zip(widths).zip(ALIGN) {
        let pad = width - cell.width();
        let (before, after) = match align {
            Align::Left => (0, pad),
            Align::Right => (pad, 0),
        };
        write!(out, "{:n$}{cell}", "", n = owed + before)?;
        owed = after + GAP;
    }
    writeln!(out)
}

fn main() -> io::Result<()> {
    let stdout = io::stdout();
    let layout = if stdout.is_terminal() {
        Layout::Table
    } else {
        Layout::Tsv
    };
    let entries = read_entries(Path::new("."))?;
    render(&mut stdout.lock(), layout, &entries, SystemTime::now())
}

#[cfg(test)]
mod tests;
