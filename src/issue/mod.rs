//! Validation findings.
//!
//! An [`Issue`] is generic over what it is about, so a tool with more kinds
//! of files than profiles (a `config.toml`, Steam games by app id) can use its
//! own subject type and still collect with [`Issues`]; issues from this crate
//! convert with [`Issue::map_subject`].

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use crate::{ShaderCache, expand_tilde, validate_name};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Loads and runs, but probably not as meant.
    Warning,
    /// Can't run like this.
    Error,
}

impl Severity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What an [`Issue`] is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subject {
    /// A game, by name.
    Game(String),
    /// A profile, by name.
    Profile(String),
    /// A file that could not be loaded at all.
    File(PathBuf),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game(name) => write!(f, "game `{name}`"),
            Self::Profile(name) => write!(f, "profile `{name}`"),
            Self::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// One finding: `error: profile `default`: `prefix` must be an absolute path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue<S = Subject> {
    pub severity: Severity,
    pub subject: S,
    pub message: String,
}

impl<S> Issue<S> {
    pub fn error(subject: S, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            subject,
            message: message.into(),
        }
    }

    pub fn warning(subject: S, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            subject,
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// The same finding about another kind of subject, e.g. a tool's own
    /// subject type: `issue.map_subject(Into::into)`.
    pub fn map_subject<T>(self, f: impl FnOnce(S) -> T) -> Issue<T> {
        Issue {
            severity: self.severity,
            subject: f(self.subject),
            message: self.message,
        }
    }
}

impl<S: fmt::Display> fmt::Display for Issue<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.severity, self.subject, self.message)
    }
}

/// Collects issues about one subject, with the checks every file format of
/// the tools shares.
#[derive(Debug, Clone)]
pub struct Issues<S = Subject> {
    subject: S,
    list: Vec<Issue<S>>,
}

impl<S: Clone> Issues<S> {
    pub fn new(subject: S) -> Self {
        Self {
            subject,
            list: Vec::new(),
        }
    }

    pub fn subject(&self) -> &S {
        &self.subject
    }

    pub fn push(&mut self, severity: Severity, message: impl Into<String>) {
        self.list.push(Issue {
            severity,
            subject: self.subject.clone(),
            message: message.into(),
        });
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(Severity::Error, message);
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.push(Severity::Warning, message);
    }

    /// Errors unless `name` passes [`validate_name`].
    pub fn name(&mut self, name: &str) {
        if let Err(message) = validate_name(name) {
            self.error(message);
        }
    }

    /// Errors unless `path` is absolute after `~` expansion; returns the
    /// expanded path when it is.
    pub fn absolute(&mut self, field: &str, path: &Path, home: &Path) -> Option<PathBuf> {
        let expanded = expand_tilde(path, home);
        if expanded.is_absolute() {
            return Some(expanded);
        }
        self.error(format!(
            "`{field}` must be an absolute path (or start with `~`), got `{}`",
            path.display()
        ));
        None
    }

    /// Errors for variable names a process environment can't hold, and for
    /// values with a NUL byte (which no process can be started with).
    pub fn env(&mut self, field: &str, env: &BTreeMap<String, String>) {
        for (key, value) in env {
            if key.is_empty() || key.contains(['=', '\0']) {
                self.error(format!(
                    "`{field}` key `{key}` is not a valid variable name"
                ));
            } else if value.contains('\0') {
                self.error(format!("`{field}.{key}` must not contain a NUL character"));
            }
        }
    }

    /// Errors for arguments a process can't be started with: those with a NUL
    /// byte, and (with `allow_empty` off) blank ones.
    pub fn args(&mut self, field: &str, args: &[String], allow_empty: bool) {
        if !allow_empty && args.iter().any(|arg| arg.trim().is_empty()) {
            self.error(format!("`{field}` must not contain empty entries"));
        }
        if args.iter().any(|arg| arg.contains('\0')) {
            self.error(format!("`{field}` must not contain a NUL character"));
        }
    }

    /// Checks a `shader_cache` value: `off`, or an absolute directory that
    /// can be created (not below an existing file).
    pub fn shader_cache(&mut self, field: &str, value: &ShaderCache, home: &Path) {
        let ShaderCache::Dir(dir) = value else {
            return;
        };
        if let Some(dir) = self.absolute(field, dir, home)
            && let Some(existing) = dir.ancestors().find(|dir| dir.exists())
            && !existing.is_dir()
        {
            self.error(format!(
                "`{field}` can't be created: `{}` is not a directory",
                existing.display()
            ));
        }
    }

    /// Adds issues found elsewhere, e.g. by [`crate::Profile::validate`].
    pub fn extend(&mut self, issues: impl IntoIterator<Item = Issue<S>>) {
        self.list.extend(issues);
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[must_use]
    pub fn finish(self) -> Vec<Issue<S>> {
        self.list
    }
}
