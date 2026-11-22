//! Profile directories: `<dir>/<name>.toml`, one profile per file.
//!
//! Loading is forgiving: a file that can't be loaded becomes an [`Error`] in
//! [`Loaded::errors`] and the others still load, and keys the schema doesn't
//! know are warnings, not errors, so a file written by a newer tool still
//! loads. Writes are atomic: a temporary file next to the target, synced, then
//! renamed over it.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    issue::{Issue, Issues, Subject},
    profile::Profile,
    validate_name,
};

#[cfg(test)]
mod tests;

/// Every key a profile file may have, as dotted paths; `.*` accepts any key
/// below a table.
pub const KNOWN_KEYS: &[&str] = &[
    "name",
    "runner",
    "prefix",
    "runner_args",
    "wrappers",
    "debug",
    "dxvk",
    "vkd3d",
    "env.*",
    "dll_overrides.*",
    "wine",
    "wine.path",
    "umu",
    "umu.umu_run",
    "umu.proton",
    "umu.game_id",
    "umu.store",
    "proton",
    "proton.build",
    "steam",
    "anticheat",
    "anticheat.battleye",
    "anticheat.eac",
    "shader_cache",
    "slr",
    "slr.runtime",
    "slr.filesystems_rw",
    "slr.filesystems_ro",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },

    #[error("invalid {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },

    #[error("failed to serialize {}: {source}", path.display())]
    Serialize {
        path: PathBuf,
        source: toml::ser::Error,
    },

    #[error("{}: `name = \"{name}\"` must match the file name `{stem}`", path.display())]
    NameMismatch {
        path: PathBuf,
        name: String,
        stem: String,
    },

    #[error("invalid profile name: {0}")]
    InvalidName(String),
}

impl Error {
    /// The file this error is about, if any.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Read { path, .. }
            | Self::Write { path, .. }
            | Self::Parse { path, .. }
            | Self::Serialize { path, .. }
            | Self::NameMismatch { path, .. } => Some(path),
            Self::InvalidName(_) => None,
        }
    }

    /// The error without the file path, for when the path is shown separately.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Read { source, .. } => format!("failed to read: {source}"),
            Self::Write { source, .. } => format!("failed to write: {source}"),
            Self::Parse { source, .. } => format!("invalid TOML: {source}"),
            Self::Serialize { source, .. } => format!("failed to serialize: {source}"),
            Self::NameMismatch { name, stem, .. } => {
                format!("`name = \"{name}\"` must match the file name `{stem}`")
            }
            Self::InvalidName(message) => message.clone(),
        }
    }

    /// This error as an [`Issue`] about its file.
    pub fn to_issue(&self) -> Issue {
        Issue::error(
            Subject::File(self.path().map_or_else(PathBuf::new, Path::to_path_buf)),
            self.detail(),
        )
    }
}

/// `<dir>/<name>.toml`. `name` isn't checked: see [`validate_name`].
#[must_use]
pub fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.toml"))
}

/// Parses one profile file's text. Keys the schema doesn't know become
/// warnings about `subject`.
///
/// # Errors
///
/// The text isn't TOML, or not a profile (a missing or mistyped field).
pub fn parse(text: &str, subject: Subject) -> Result<(Profile, Vec<Issue>), toml::de::Error> {
    let profile: Profile = toml::from_str(text)?;
    let table: toml::Table = toml::from_str(text)?;
    let mut issues = Issues::new(subject);
    unknown_keys(&table, "", &mut issues);
    Ok((profile, issues.finish()))
}

/// Loads one profile file, whose `name` must match its file stem.
///
/// # Errors
///
/// [`Error::Read`], [`Error::Parse`] or [`Error::NameMismatch`].
pub fn load(path: &Path) -> Result<(Profile, Vec<Issue>), Error> {
    let text = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (profile, issues) =
        parse(&text, Subject::Profile(stem.clone())).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
    if profile.name != stem {
        return Err(Error::NameMismatch {
            path: path.to_path_buf(),
            name: profile.name,
            stem,
        });
    }
    Ok((profile, issues))
}

/// A profile directory as loaded.
#[derive(Debug, Default)]
pub struct Loaded {
    /// The files that loaded, sorted by name.
    pub profiles: Vec<Profile>,
    /// Unknown keys in the files that loaded.
    pub warnings: Vec<Issue>,
    /// The files (or the directory) that didn't load.
    pub errors: Vec<Error>,
}

impl Loaded {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    /// The load errors and warnings as issues, errors first.
    pub fn issues(&self) -> impl Iterator<Item = Issue> + '_ {
        self.errors
            .iter()
            .map(Error::to_issue)
            .chain(self.warnings.iter().cloned())
    }
}

/// Loads every `*.toml` file in `dir`; a missing directory has no profiles.
#[must_use]
pub fn load_dir(dir: &Path) -> Loaded {
    let mut loaded = Loaded::default();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return loaded,
        Err(source) => {
            loaded.errors.push(Error::Read {
                path: dir.to_path_buf(),
                source,
            });
            return loaded;
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "toml") && path.is_file() {
                    paths.push(path);
                }
            }
            Err(source) => loaded.errors.push(Error::Read {
                path: dir.to_path_buf(),
                source,
            }),
        }
    }
    paths.sort();
    for path in paths {
        match load(&path) {
            Ok((profile, mut warnings)) => {
                loaded.warnings.append(&mut warnings);
                loaded.profiles.push(profile);
            }
            Err(err) => loaded.errors.push(err),
        }
    }
    loaded.profiles.sort_by(|a, b| a.name.cmp(&b.name));
    loaded
}

/// Writes `<dir>/<name>.toml` atomically, creating `dir`. A symlinked file is
/// written through (the link stays), and an existing file keeps its
/// permissions.
///
/// # Errors
///
/// [`Error::InvalidName`] before anything is written, [`Error::Serialize`],
/// or [`Error::Write`] (the old file, if any, is then unchanged).
pub fn save(dir: &Path, profile: &Profile) -> Result<PathBuf, Error> {
    validate_name(&profile.name).map_err(Error::InvalidName)?;
    let path = path(dir, &profile.name);
    let text = toml::to_string_pretty(profile).map_err(|source| Error::Serialize {
        path: path.clone(),
        source,
    })?;
    write_atomic(&path, &text)?;
    Ok(path)
}

/// Deletes `<dir>/<name>.toml`. Nothing else (the prefix, caches) is touched.
///
/// # Errors
///
/// [`Error::InvalidName`], or [`Error::Write`] (also when there is no such
/// file).
pub fn remove(dir: &Path, name: &str) -> Result<(), Error> {
    validate_name(name).map_err(Error::InvalidName)?;
    let path = path(dir, name);
    fs::remove_file(&path).map_err(|source| Error::Write { path, source })
}

/// Writes `text` to `path` through a synced temporary file and a rename.
///
/// Public so tools write their other files (`games.toml`) the same way.
///
/// # Errors
///
/// [`Error::Write`]; `path` is then unchanged and no temporary file is left.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), Error> {
    // Unique per process and per write, so concurrent saves (from threads or
    // processes) never share a temporary file. It doesn't end in `.toml`, so
    // one left by a crash is never loaded.
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let err = |source| Error::Write {
        path: path.to_path_buf(),
        source,
    };
    let target = resolve_links(path).map_err(err)?;
    let dir = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    fs::create_dir_all(dir).map_err(err)?;
    let permissions = fs::metadata(&target).ok().map(|meta| meta.permissions());

    let mut tmp_name = target.file_name().unwrap_or_default().to_owned();
    tmp_name.push(format!(
        ".tmp.{}.{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = dir.join(tmp_name);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        io::Write::write_all(&mut file, text.as_bytes())?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.sync_all()?;
        fs::rename(&tmp, &target)?;
        // Make the rename itself durable; best effort, as not every file
        // system can sync a directory.
        if let Ok(dir) = fs::File::open(dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(err)
}

/// Where a write to `path` should go: through symlinks (also dangling ones)
/// to the file they name, so a profile directory of links into a dotfiles
/// checkout keeps its links.
fn resolve_links(path: &Path) -> io::Result<PathBuf> {
    let mut path = path.to_path_buf();
    // Linux's own limit on links followed in one lookup.
    for _ in 0..40 {
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = fs::read_link(&path)?;
                path = match path.parent() {
                    Some(parent) => parent.join(link),
                    None => link,
                };
            }
            _ => return Ok(path),
        }
    }
    Err(io::Error::other("too many levels of symbolic links"))
}

/// Warns about every key of `table` (at `prefix`) not in [`KNOWN_KEYS`].
fn unknown_keys(table: &toml::Table, prefix: &str, issues: &mut Issues) {
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if KNOWN_KEYS.contains(&format!("{path}.*").as_str()) {
            continue;
        }
        if !KNOWN_KEYS.contains(&path.as_str()) {
            issues.warn(format!("unknown key `{path}` is ignored"));
        } else if let toml::Value::Table(table) = value {
            unknown_keys(table, &path, issues);
        }
    }
}
