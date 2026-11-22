//! Every profile under `examples/profiles/` loads and validates cleanly.

use std::path::Path;

use libgameprofiles::store;

#[test]
fn example_profiles_are_clean() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/profiles");
    let loaded = store::load_dir(&dir);
    let issues: Vec<String> = loaded.issues().map(|i| i.to_string()).collect();
    assert!(issues.is_empty(), "{issues:#?}");
    assert_eq!(
        loaded
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["native", "proton", "slr", "umu", "wine"]
    );
    for profile in &loaded.profiles {
        let issues: Vec<String> = profile
            .validate(Path::new("/home/user"))
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(issues.is_empty(), "{}: {issues:#?}", profile.name);
    }
}
