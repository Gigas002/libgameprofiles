use std::path::Path;

use super::*;

#[test]
fn valid_names() {
    for name in ["wow", "Witcher-3", "game_1.2", "a", ".hidden"] {
        assert_eq!(validate_name(name), Ok(()), "{name}");
    }
}

#[test]
fn invalid_names() {
    for name in [
        "", ".", "..", "-wow", "my game", "a/b", "wöw", "a\0b", "a\\b",
    ] {
        assert!(validate_name(name).is_err(), "{name:?}");
    }
}

#[test]
fn tilde_expansion() {
    let home = Path::new("/home/u");
    assert_eq!(expand_tilde(Path::new("~"), home), home);
    assert_eq!(expand_tilde(Path::new("~/"), home), home);
    assert_eq!(
        expand_tilde(Path::new("~/a/b"), home),
        Path::new("/home/u/a/b")
    );
    assert_eq!(
        expand_tilde(Path::new("~user/a"), home),
        Path::new("~user/a")
    );
    assert_eq!(expand_tilde(Path::new("/a/~"), home), Path::new("/a/~"));
    assert_eq!(expand_tilde(Path::new("a"), home), Path::new("a"));
}
