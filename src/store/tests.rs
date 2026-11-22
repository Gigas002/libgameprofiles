use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use super::*;
use crate::{Runner, Severity, profile::SlrSection};

fn write(dir: &Path, file: &str, text: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(file), text).unwrap();
}

#[test]
fn missing_dir_has_no_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = load_dir(&dir.path().join("missing"));
    assert!(loaded.profiles.is_empty() && loaded.errors.is_empty() && loaded.warnings.is_empty());
}

#[test]
fn load_dir_keeps_going_past_bad_files() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    write(dir, "b.toml", "name = \"b\"\nrunner = \"native\"\n");
    write(
        dir,
        "a.toml",
        "name = \"a\"\nrunner = \"wine\"\nprefix = \"/p\"\n",
    );
    write(dir, "broken.toml", "name = \"broken\"\nrunner = ");
    write(
        dir,
        "renamed.toml",
        "name = \"other\"\nrunner = \"native\"\n",
    );
    write(dir, "notes.txt", "not a profile");
    write(dir, "x.toml.tmp.1", "name = \"x\"\nrunner = \"native\"\n");
    fs::create_dir(dir.join("dir.toml")).unwrap();

    let loaded = load_dir(dir);
    let names: Vec<_> = loaded.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
    assert_eq!(loaded.get("a").unwrap().runner, Runner::Wine);
    assert!(loaded.get("broken").is_none() && loaded.get("c").is_none());
    assert_eq!(loaded.errors.len(), 2, "{:?}", loaded.errors);
    assert!(matches!(loaded.errors[0], Error::Parse { .. }));
    assert!(matches!(
        &loaded.errors[1],
        Error::NameMismatch { name, stem, .. } if name == "other" && stem == "renamed"
    ));

    let issues: Vec<_> = loaded.issues().collect();
    assert_eq!(issues.len(), 2);
    assert!(issues.iter().all(|issue| issue.severity == Severity::Error));
    assert_eq!(issues[1].subject, Subject::File(dir.join("renamed.toml")));
    assert_eq!(
        issues[1].message,
        "`name = \"other\"` must match the file name `renamed`"
    );
}

#[test]
fn parse_errors_point_at_the_line() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "p.toml",
        "name = \"p\"\n\nrunner = \"wine\"\ndebug = \"yes\"\n",
    );
    let err = load(&dir.path().join("p.toml")).unwrap_err();
    assert!(err.to_string().contains("line 4"), "{err}");
    assert!(
        err.detail()
            .starts_with("invalid TOML: TOML parse error at line 4"),
        "{}",
        err.detail()
    );
}

#[test]
fn unknown_keys_are_warnings() {
    let text = r#"
name = "p"
runner = "native"
runer = "wine"

[env]
ANY_NAME = "is fine"

[dll_overrides]
any = "n"

[slr]
runtime = "sniper"
filesystem_rw = ["/x"]

[anticheat]
eac = true
vac = true

[future]
key = 1
"#;
    let (profile, issues) = parse(text, Subject::Profile("p".into())).unwrap();
    assert_eq!(profile.slr, Some(SlrSection::new("sniper")));
    let messages: Vec<_> = issues.iter().map(|issue| issue.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "unknown key `anticheat.vac` is ignored",
            "unknown key `future` is ignored",
            "unknown key `runer` is ignored",
            "unknown key `slr.filesystem_rw` is ignored",
        ]
    );
    assert!(
        issues
            .iter()
            .all(|issue| issue.severity == Severity::Warning)
    );
    assert_eq!(issues[0].subject, Subject::Profile("p".into()));
}

#[test]
fn every_known_key_is_a_field() {
    // A file with every known key has no unknown ones, and parses.
    let text = r#"
name = "all"
runner = "wine"
prefix = "/p"
runner_args = []
wrappers = []
debug = false
dxvk = "/d"
vkd3d = "/v"
shader_cache = "off"
steam = "auto"
env = {}
dll_overrides = {}
wine = { path = "/w" }
umu = { umu_run = "/u", proton = "/p", game_id = "g", store = "s" }
proton = { build = "b" }
anticheat = { battleye = false, eac = false }
slr = { runtime = "r", mode = "overlay", filesystems_rw = [], filesystems_ro = [], lib_overrides = {} }
"#;
    let (_, issues) = parse(text, Subject::Profile("all".into())).unwrap();
    assert_eq!(issues, Vec::new());
    let table: toml::Table = toml::from_str(text).unwrap();
    let mut keys = Vec::new();
    for (key, value) in &table {
        match value {
            toml::Value::Table(inner) if !inner.is_empty() => {
                keys.push(key.clone());
                keys.extend(inner.iter().map(|(k, v)| match v {
                    toml::Value::Table(_) => format!("{key}.{k}.*"),
                    _ => format!("{key}.{k}"),
                }));
            }
            toml::Value::Table(_) => keys.push(format!("{key}.*")),
            _ => keys.push(key.clone()),
        }
    }
    keys.sort();
    let mut known: Vec<_> = KNOWN_KEYS.iter().map(|key| (*key).to_owned()).collect();
    known.sort();
    assert_eq!(keys, known);
}

#[test]
fn save_load_remove() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles");
    let mut profile = Profile::new("p", Runner::Native);
    profile.slr = Some(SlrSection::new("sniper"));

    let path = save(&profiles, &profile).unwrap();
    assert_eq!(path, profiles.join("p.toml"));
    assert_eq!(load(&path).unwrap(), (profile.clone(), Vec::new()));
    // Only the profile is left: no temporary files.
    assert_eq!(fs::read_dir(&profiles).unwrap().count(), 1);

    profile.debug = true;
    save(&profiles, &profile).unwrap();
    assert!(load(&path).unwrap().0.debug);

    remove(&profiles, "p").unwrap();
    assert!(!path.exists());
    assert!(matches!(
        remove(&profiles, "p"),
        Err(Error::Write { source, .. }) if source.kind() == io::ErrorKind::NotFound
    ));
}

#[test]
fn names_that_leave_the_directory_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["../escape", "a/b", "", ".."] {
        assert!(matches!(
            save(dir.path(), &Profile::new(name, Runner::Native)),
            Err(Error::InvalidName(_))
        ));
        assert!(matches!(
            remove(dir.path(), name),
            Err(Error::InvalidName(_))
        ));
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn save_writes_through_symlinks_and_keeps_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    let profiles = dir.path().join("profiles");
    write(&dotfiles, "p.toml", "name = \"p\"\nrunner = \"wine\"\n");
    fs::set_permissions(dotfiles.join("p.toml"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::create_dir(&profiles).unwrap();
    std::os::unix::fs::symlink("../dotfiles/p.toml", profiles.join("p.toml")).unwrap();
    // A dangling link is written through too, creating its target.
    std::os::unix::fs::symlink("../dotfiles/new/q.toml", profiles.join("q.toml")).unwrap();

    save(&profiles, &Profile::new("p", Runner::Native)).unwrap();
    save(&profiles, &Profile::new("q", Runner::Native)).unwrap();

    for name in ["p.toml", "q.toml"] {
        assert!(
            fs::symlink_metadata(profiles.join(name))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
    let text = fs::read_to_string(dotfiles.join("p.toml")).unwrap();
    assert!(text.contains("runner = \"native\""), "{text}");
    assert!(dotfiles.join("new/q.toml").is_file());
    let mode = fs::metadata(dotfiles.join("p.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    // Nothing left behind next to either target.
    assert_eq!(fs::read_dir(&dotfiles).unwrap().count(), 2);
    assert_eq!(fs::read_dir(dotfiles.join("new")).unwrap().count(), 1);
}

#[test]
fn a_symlink_loop_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("b.toml", dir.path().join("a.toml")).unwrap();
    std::os::unix::fs::symlink("a.toml", dir.path().join("b.toml")).unwrap();
    assert!(matches!(
        save(dir.path(), &Profile::new("a", Runner::Native)),
        Err(Error::Write { .. })
    ));
}

#[test]
fn a_failed_write_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    // The target is a directory: the rename fails, the temporary file goes.
    fs::create_dir(dir.path().join("p.toml")).unwrap();
    assert!(matches!(
        save(dir.path(), &Profile::new("p", Runner::Native)),
        Err(Error::Write { .. })
    ));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn unreadable_dir_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    fs::write(&file, "").unwrap();
    let loaded = load_dir(&file);
    assert_eq!(loaded.errors.len(), 1);
    assert_eq!(loaded.errors[0].path(), Some(file.as_path()));
}

#[test]
fn unreadable_file_is_a_read_error() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "p.toml", "name = \"p\"\nrunner = \"native\"\n");
    let path = dir.path().join("p.toml");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&path).is_ok() {
        return; // root reads anything
    }
    let loaded = load_dir(dir.path());
    assert!(loaded.profiles.is_empty());
    assert!(matches!(&loaded.errors[..], [Error::Read { .. }]));
    assert!(loaded.errors[0].detail().starts_with("failed to read: "));
}

#[test]
fn error_texts() {
    let path = PathBuf::from("/p/x.toml");
    let io = || io::Error::from(io::ErrorKind::PermissionDenied);
    let read = Error::Read {
        path: path.clone(),
        source: io(),
    };
    assert_eq!(
        read.to_string(),
        format!("failed to read /p/x.toml: {}", io())
    );
    assert_eq!(read.detail(), format!("failed to read: {}", io()));
    let write = Error::Write {
        path: path.clone(),
        source: io(),
    };
    assert_eq!(write.detail(), format!("failed to write: {}", io()));
    assert_eq!(write.path(), Some(path.as_path()));
    let invalid = Error::InvalidName("name `a b` contains ` `".into());
    assert_eq!(invalid.path(), None);
    assert_eq!(invalid.detail(), "name `a b` contains ` `");
    assert_eq!(invalid.to_issue().subject, Subject::File(PathBuf::new()));
    assert_eq!(
        invalid.to_string(),
        "invalid profile name: name `a b` contains ` `"
    );
}

#[test]
fn concurrent_saves_never_clobber_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    std::thread::scope(|scope| {
        for thread in 0..8 {
            scope.spawn(move || {
                for _ in 0..50 {
                    let mut profile = Profile::new("p", Runner::Native);
                    profile.wrappers = vec![format!("wrapper-{thread}"); 50];
                    save(dir, &profile).unwrap();
                    // Always one whole file, from one thread or another.
                    let (loaded, _) = load(&dir.join("p.toml")).unwrap();
                    assert_eq!(loaded.wrappers.len(), 50);
                }
            });
        }
    });
    assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
}

#[test]
fn deep_nesting_is_an_error_not_a_crash() {
    let text = format!(
        "name = \"p\"\nrunner = \"native\"\nwrappers = {}{}",
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    assert!(parse(&text, Subject::Profile("p".into())).is_err());
}
