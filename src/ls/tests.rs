use super::*;
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
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    relative_time(now - Duration::from_secs(secs), now)
}

fn ahead(secs: u64) -> String {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    relative_time(now + Duration::from_secs(secs), now)
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

/*
const HOUR: u64 = 3600;

fn now() -> SystemTime {

} */
