use std::ffi::OsStr;
use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime};

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;

// left in target/tmp on purpose: when a test fails, go look at it
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn file(dir: &Path, name: impl AsRef<Path>, len: usize, age: u64) {
    let path = dir.join(name);
    fs::write(&path, vec![b'x'; len]).unwrap();
    let mtime = SystemTime::now() - Duration::from_secs(age);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
}

fn ls(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ls"));
    cmd.current_dir(dir).args(args);
    cmd
}

fn run(dir: &Path, args: &[&str]) -> Output {
    ls(dir, args).output().unwrap()
}

fn stdout(out: &Output) -> &str {
    std::str::from_utf8(&out.stdout).unwrap()
}

fn stderr(out: &Output) -> &str {
    std::str::from_utf8(&out.stderr).unwrap()
}

#[test]
fn empty_dir_prints_nothing_when_piped() {
    let dir = fixture("empty_dir_prints_nothing_when_piped");
    let out = run(&dir, &[]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
}

#[test]
fn lists_cwd_as_tsv_when_piped() {
    let dir = fixture("lists_cwd_as_tsv_when_piped");
    fs::create_dir(dir.join("src")).unwrap();
    fs::create_dir(dir.join("Docs")).unwrap();
    fs::create_dir(dir.join(".git")).unwrap();
    file(&dir, "Cargo.toml", 312, 3 * DAY);
    file(&dir, "README.md", 2048, 2 * HOUR);
    file(&dir, "notes.txt", 0, 10 * 60);
    file(&dir, ".env", 10, 0);
    symlink("Cargo.toml", dir.join("link")).unwrap();

    let out = run(&dir, &[]);

    assert!(out.status.success(), "{out:?}");
    assert_eq!(stderr(&out), "");
    assert_eq!(
        stdout(&out),
        "\
Docs\tdir\t-\tjust now
src\tdir\t-\tjust now
Cargo.toml\tfile\t312 B\t3 days ago
link\tlink\t-\tjust now
notes.txt\tfile\t0 B\t10 minutes ago
README.md\tfile\t2.0 KiB\t2 hours ago
"
    );
}

#[test]
fn weird_names_cannot_break_rows() {
    let dir = fixture("weird_names_cannot_break_rows");
    file(&dir, "evil\nname", 0, 0);
    file(&dir, "tab\there", 0, 0);
    file(&dir, OsStr::from_bytes(b"bad\xffbyte"), 0, 0);

    let out = run(&dir, &[]);

    assert!(out.status.success());
    assert_eq!(
        stdout(&out),
        "\
bad\\xffbyte\tfile\t0 B\tjust now
evil\\x0aname\tfile\t0 B\tjust now
tab\\x09here\tfile\t0 B\tjust now
"
    );
}

#[test]
fn future_mtime() {
    let dir = fixture("future_mtime");
    let path = dir.join("from-the-future");
    fs::write(&path, b"").unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(2 * DAY + HOUR))
        .unwrap();

    let out = run(&dir, &[]);

    assert_eq!(stdout(&out), "from-the-future\tfile\t0 B\tin 2 days\n");
}

#[test]
fn write_error_is_reported() {
    let dir = fixture("write_error_is_reported");
    file(&dir, "a", 0, 0);
    let full = File::options().write(true).open("/dev/full").unwrap();

    let out = ls(&dir, &[]).stdout(full).output().unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        "ls: write error: No space left on device (os error 28)\n"
    );
}

#[test]
fn broken_pipe_is_quiet() {
    let dir = fixture("broken_pipe_is_quiet");
    // well past the 64 KiB pipe buffer, so ls is still writing when the
    // reader goes away
    for i in 0..10_000 {
        fs::write(dir.join(format!("file-{i:05}")), b"").unwrap();
    }

    let mut child = ls(&dir, &[])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();

    assert!(out.status.success(), "{out:?}");
    assert_eq!(stderr(&out), "");
}

#[test]
fn unreadable_dir_is_an_error() {
    let dir = fixture("unreadable_dir_is_an_error");
    file(&dir, "a", 0, 0);
    let mode = |m| fs::set_permissions(&dir, fs::Permissions::from_mode(m)).unwrap();
    // search but no read: ls can chdir into it, but not list it
    mode(0o100);
    let readable_anyway = fs::read_dir(&dir).is_ok();
    let out = run(&dir, &[]);
    mode(0o755);
    if readable_anyway {
        // root ignores the mode bits, there is nothing to test
        return;
    }

    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        stderr(&out),
        "ls: cannot read directory '.': Permission denied (os error 13)\n"
    );
}

#[test]
fn help_short_and_long_are_identical() {
    let dir = fixture("help_short_and_long_are_identical");
    let short = run(&dir, &["-h"]);
    let long = run(&dir, &["--help"]);

    assert!(short.status.success());
    assert!(stdout(&short).starts_with("usage: ls"));
    assert_eq!(stderr(&short), "");
    assert_eq!(short.stdout, long.stdout);
}

#[test]
fn unknown_option_is_usage_error() {
    let dir = fixture("unknown_option_is_usage_error");
    let out = run(&dir, &["-l"]);

    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        stderr(&out),
        "ls: unrecognized option '-l'\nTry 'ls --help' for more information.\n"
    );
}

#[test]
fn path_argument_is_usage_error() {
    let dir = fixture("path_argument_is_usage_error");
    let out = run(&dir, &["src"]);

    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        "ls: unexpected argument 'src' (paths are not supported yet)\n\
         Try 'ls --help' for more information.\n"
    );
}
