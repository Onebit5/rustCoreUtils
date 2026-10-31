use super::*;
use std::os::unix::ffi::OsStringExt;
use std::time::Duration;

fn entry(name: &str, kind: Kind) -> Entry {
    Entry {
        name: name.into(),
        kind,
        meta: Err(io::ErrorKind::NotFound.into()),
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

#[test]
fn size_bytes() {
    assert_eq!(human_size(0), "0 B");
    assert_eq!(human_size(312), "312 B");
    assert_eq!(human_size(1023), "1023 B");
}

#[test]
fn size_units() {
    assert_eq!(human_size(1024), "1.0 KiB");
    assert_eq!(human_size(1536), "1.5 KiB");
    assert_eq!(human_size(3 << 30), "3.0 GiB");
}

#[test]
fn size_rounds_up_into_next_unit() {
    assert_eq!(human_size(1024 * 1024 - 1), "1.0 MiB");
}

#[test]
fn size_max() {
    assert_eq!(human_size(u64::MAX), "16.0 EiB");
}

fn ago(secs: u64) -> String {
    relative_time(now() - Duration::from_secs(secs), now())
}

fn ahead(secs: u64) -> String {
    relative_time(now() + Duration::from_secs(secs), now())
}

#[test]
fn time_just_now() {
    assert_eq!(ago(0), "just now");
    assert_eq!(ago(59), "just now");
    assert_eq!(ahead(30), "just now");
}

#[test]
fn time_singular_and_plural() {
    assert_eq!(ago(60), "1 minute ago");
    assert_eq!(ago(2 * 3600 + 59), "2 hours ago");
    assert_eq!(ago(86400), "1 day ago");
    assert_eq!(ago(21 * 86400), "3 weeks ago");
    assert_eq!(ago(45 * 86400), "1 month ago");
    assert_eq!(ago(400 * 86400), "1 year ago");
}

#[test]
fn time_future() {
    assert_eq!(ahead(2 * 86400), "in 2 days");
}

const HOUR: u64 = 3600;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000)
}

fn with_meta(name: &str, kind: Kind, len: u64, age: u64) -> Entry {
    Entry {
        name: name.into(),
        kind,
        meta: Ok(Meta {
            len,
            modified: now() - Duration::from_secs(age),
        }),
    }
}

fn show(layout: Layout, entries: &[Entry]) -> String {
    let mut out = Vec::new();
    render(&mut out, layout, entries, now()).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn table_aligns_columns() {
    let entries = [
        with_meta("src", Kind::Dir, 4096, 300),
        with_meta("Cargo.lock", Kind::File, 1234, 2 * HOUR),
        with_meta("Cargo.toml", Kind::File, 312, 2 * HOUR),
    ];
    assert_eq!(
        show(Layout::Table, &entries),
        "\
name        type     size  modified
──────────  ────  ───────  ─────────────
src         dir         -  5 minutes ago
Cargo.lock  file  1.2 KiB  2 hours ago
Cargo.toml  file    312 B  2 hours ago
"
    );
}

#[test]
fn table_measures_display_width_not_bytes() {
    let entries = [
        with_meta("日本語.txt", Kind::File, 0, 0),
        with_meta("a.txt", Kind::File, 0, 0),
    ];
    assert_eq!(
        show(Layout::Table, &entries),
        "\
name        type  size  modified
──────────  ────  ────  ────────
日本語.txt  file   0 B  just now
a.txt       file   0 B  just now
"
    );
}

#[test]
fn table_of_empty_dir_is_just_the_header() {
    assert_eq!(
        show(Layout::Table, &[]),
        "\
name  type  size  modified
────  ────  ────  ────────
"
    );
}

#[test]
fn unreadable_meta_shows_question_marks() {
    let entries = [Entry {
        name: "secret".into(),
        kind: Kind::File,
        meta: Err(io::ErrorKind::PermissionDenied.into()),
    }];
    assert_eq!(
        show(Layout::Table, &entries),
        "\
name    type  size  modified
──────  ────  ────  ────────
secret  file     ?  ?
"
    );
}

#[test]
fn tsv_has_no_header_and_escapes_tabs() {
    let entries = [
        with_meta("src", Kind::Dir, 4096, 300),
        with_meta("a\tb", Kind::File, 2048, 2 * HOUR),
    ];
    assert_eq!(
        show(Layout::Tsv, &entries),
        "src\tdir\t-\t5 minutes ago\na\\x09b\tfile\t2.0 KiB\t2 hours ago\n"
    );
}

fn esc(bytes: &[u8]) -> String {
    escape_name(&OsString::from_vec(bytes.to_vec()))
}

#[test]
fn escape_leaves_normal_names_alone() {
    assert_eq!(esc(b"Cargo.toml"), "Cargo.toml");
    assert_eq!(esc("日本語.txt".as_bytes()), "日本語.txt");
    assert_eq!(esc(b"with space"), "with space");
}

#[test]
fn escape_control_and_backslash() {
    assert_eq!(esc(b"evil\nname"), "evil\\x0aname");
    assert_eq!(esc(b"tab\there"), "tab\\x09here");
    assert_eq!(esc(b"\x1b[31mred"), "\\x1b[31mred");
    assert_eq!(esc(b"back\\slash"), "back\\\\slash");
}

#[test]
fn escape_c1_control_as_utf8_bytes() {
    assert_eq!(esc("\u{9b}".as_bytes()), "\\xc2\\x9b");
}

#[test]
fn escape_invalid_utf8() {
    assert_eq!(esc(b"bad\xffname"), "bad\\xffname");
}

fn parse_strs(args: &[&str]) -> Result<Command, UsageError> {
    parse_args(args.iter().map(OsString::from))
}

#[test]
fn no_args_lists() {
    assert_eq!(parse_strs(&[]), Ok(Command::List));
}

#[test]
fn help_short_and_long() {
    assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
    assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
}

#[test]
fn help_wins_over_what_follows() {
    assert_eq!(parse_strs(&["--help", "--bogus"]), Ok(Command::Help));
}

#[test]
fn unknown_options_are_errors_not_ignored() {
    for opt in ["-l", "-la", "-hl", "--all", "--h"] {
        assert_eq!(
            parse_strs(&[opt]),
            Err(UsageError::UnrecognizedOption(opt.into())),
            "{opt}"
        );
    }
}

#[test]
fn paths_are_not_supported_yet() {
    assert_eq!(
        parse_strs(&["src"]),
        Err(UsageError::UnexpectedArgument("src".into()))
    );
    assert_eq!(
        parse_strs(&["-"]),
        Err(UsageError::UnexpectedArgument("-".into()))
    );
}

#[test]
fn double_dash_ends_options() {
    assert_eq!(parse_strs(&["--"]), Ok(Command::List));
    assert_eq!(
        parse_strs(&["--", "--help"]),
        Err(UsageError::UnexpectedArgument("--help".into()))
    );
}
