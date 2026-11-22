use std::{collections::BTreeMap, path::Path};

use super::*;

#[test]
fn display() {
    let issue = Issue::error(Subject::Game("wow".into()), "broken");
    assert_eq!(issue.to_string(), "error: game `wow`: broken");
    let issue = Issue::error(Subject::Profile("p".into()), "broken");
    assert_eq!(issue.to_string(), "error: profile `p`: broken");
    let issue = Issue::warning(Subject::File("/p/x.toml".into()), "odd");
    assert_eq!(issue.to_string(), "warning: /p/x.toml: odd");
    assert!(!issue.is_error());
    assert!(Severity::Warning < Severity::Error);
}

#[test]
fn map_subject_keeps_the_finding() {
    #[derive(Debug, Clone, PartialEq)]
    enum Mine {
        Config,
        Lib(Subject),
    }
    let issue = Issue::error(Subject::Profile("p".into()), "bad").map_subject(Mine::Lib);
    assert_eq!(issue.subject, Mine::Lib(Subject::Profile("p".into())));
    assert_eq!(issue.message, "bad");
    let mut issues = Issues::new(Mine::Config);
    assert_eq!(issues.subject(), &Mine::Config);
    issues.warn("w");
    issues.extend([issue]);
    let list = issues.finish();
    assert_eq!(list[0].subject, Mine::Config);
    assert_eq!(list.len(), 2);
}

#[test]
fn shared_checks() {
    let home = Path::new("/home/u");
    let mut issues = Issues::new(Subject::Profile("p".into()));
    assert_eq!(
        issues.absolute("prefix", Path::new("~/pfx"), home),
        Some("/home/u/pfx".into())
    );
    assert!(issues.is_empty());
    assert_eq!(issues.absolute("prefix", Path::new("pfx"), home), None);
    issues.name("bad name");
    issues.env(
        "env",
        &BTreeMap::from([
            ("A=B".into(), "1".into()),
            (String::new(), "1".into()),
            ("OK".into(), "x\0y".into()),
            ("FINE".into(), String::new()),
        ]),
    );
    issues.args("wrappers", &["mangohud".into(), " ".into()], false);
    issues.args("runner_args", &[String::new(), "a\0".into()], true);
    let messages: Vec<String> = issues.finish().into_iter().map(|i| i.message).collect();
    assert_eq!(
        messages,
        [
            "`prefix` must be an absolute path (or start with `~`), got `pfx`",
            "name `bad name` contains ` `; use only letters, digits, `.`, `_` and `-`",
            "`env` key `` is not a valid variable name",
            "`env` key `A=B` is not a valid variable name",
            "`env.OK` must not contain a NUL character",
            "`wrappers` must not contain empty entries",
            "`runner_args` must not contain a NUL character",
        ]
    );
}
