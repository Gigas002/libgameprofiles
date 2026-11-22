//! Game profiles shared by [lootrs], [gaki] and [sruman].
//!
//! A profile is a `profiles/<name>.toml` file that says how games run: which
//! runner (`wine`, `umu`, `proton`, `native` or `slr`), in which prefix, with which
//! environment, wrappers and runner-specific settings. One schema serves all
//! three tools, so one profile directory can be shared between them.
//!
//! - [`profile`]: the schema ([`Profile`] and its sections) and its
//!   validation, which every tool runs before its own launch checks.
//! - [`store`]: loading a profile directory, and atomic saving and removal.
//! - [`issue`]: validation findings ([`Issue`]) and a collector ([`Issues`])
//!   that tools reuse for their own files.
//!
//! The library never reads environment variables or global configuration, and
//! never fails fast while validating: it returns every [`Issue`] it finds.
//!
//! The file format, with an example of every key, is in the
//! [README](https://github.com/Gigas002/libgameprofiles#profile-files).
//!
//! ```
//! use std::path::Path;
//!
//! use libgameprofiles::{Runner, Severity, Subject, store};
//!
//! let text = "name = \"native\"\nrunner = \"native\"\nprefix = \"~/pfx\"\n";
//! let (profile, unknown) = store::parse(text, Subject::Profile("native".into()))?;
//! assert!(unknown.is_empty());
//! assert_eq!(profile.runner, Runner::Native);
//!
//! // A `native` game runs as is, without a prefix: that's a warning.
//! let issues = profile.validate(Path::new("/home/user"));
//! assert_eq!(issues[0].severity, Severity::Warning);
//! assert_eq!(
//!     issues[0].to_string(),
//!     "warning: profile `native`: `prefix` is ignored for runner `native`"
//! );
//! # Ok::<(), toml::de::Error>(())
//! ```
//!
//! [lootrs]: https://github.com/Gigas002/lootrs
//! [gaki]: https://github.com/Gigas002/gaki
//! [sruman]: https://github.com/Gigas002/sruman

use std::path::{Path, PathBuf};

pub mod issue;
pub mod profile;
pub mod store;

pub use issue::{Issue, Issues, Severity, Subject};
pub use profile::{
    Anticheat, LibOverride, ParseRunnerError, Profile, ProtonSection, Runner, ShaderCache, SlrMode,
    SlrSection, SteamIntegration, UmuSection, WineSection,
};

#[cfg(test)]
mod tests;

/// Checks a profile (or game) name: `[A-Za-z0-9._-]+`, not `.` or `..`, not
/// starting with `-`.
///
/// Names become file names, command-line arguments and process tags, so
/// nothing else is allowed.
///
/// # Errors
///
/// What's wrong with the name, naming it.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name must not be empty".into());
    }
    if name == "." || name == ".." || name.starts_with('-') {
        return Err(format!("name `{name}` is not allowed"));
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
    {
        return Err(format!(
            "name `{name}` contains `{c}`; use only letters, digits, `.`, `_` and `-`"
        ));
    }
    Ok(())
}

/// Expands a leading `~` (alone or followed by `/`) to `home`.
///
/// `~user` forms are left untouched, as is everything else.
#[must_use]
pub fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) if rest.as_os_str().is_empty() => home.to_path_buf(),
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}
