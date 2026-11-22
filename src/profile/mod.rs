//! The profile schema: `profiles/<name>.toml`.
//!
//! One [`Profile`] type holds every tool's settings. Settings that don't apply
//! to the selected runner are kept (so switching `runner` back and forth never
//! loses data) but get a warning from [`Profile::validate`].
//!
//! | Setting                                       | `wine` | `umu` | `proton` | `native` | `slr` |
//! | --------------------------------------------- | :----: | :---: | :------: | :------: | :---: |
//! | `wrappers`, `debug`, `env`, `shader_cache`    |   ✓    |   ✓   |    ✓     |    ✓     |   ✓   |
//! | `prefix`, `runner_args`                       |   ✓    |   ✓   |    ✓     |          |   ✓   |
//! | `dll_overrides`, `[anticheat]`                |   ✓    |   ✓   |    ✓     |          |       |
//! | `dxvk`, `vkd3d`, `[wine]`                     |   ✓    |       |          |          |       |
//! | `[umu]`                                       |        |   ✓   |          |          |       |
//! | `[proton]`, `steam`                           |        |       |    ✓     |          |       |
//! | `[slr]` (required)                            |        |       |          |          |   ✓   |
//!
//! An `slr` profile runs Linux games with a Steam Linux Runtime (sruman).
//! `prefix` gives the game a private home (`<prefix>/home`), and
//! [`SlrSection::mode`] says where its libraries come from: the runtime's
//! container, or the host with the runtime's libraries on top.
//!
//! [`Profile::validate`] checks the file itself: names, paths, the values a
//! process can be started with, and settings the runner ignores. What a tool
//! needs to launch a profile (a runner it supports, a required `prefix`, an
//! installed runtime) is that tool's to check.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

use crate::issue::{Issue, Issues, Subject};

#[cfg(test)]
mod tests;

/// A profile from `profiles/<name>.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Must equal the file stem.
    pub name: String,
    pub runner: Runner,
    /// The Wine prefix for `wine` and `umu`; `STEAM_COMPAT_DATA_PATH` for
    /// `proton`; the game's private home (`<prefix>/home`) for `slr`. Several
    /// profiles may share one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<PathBuf>,
    /// Inserted between the runner and the executable (for `slr`: options of
    /// the runtime's entry point).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runner_args: Vec<String>,
    /// Prepended to the whole command, e.g. `["mangohud"]`, like Steam's
    /// `mangohud %command%`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrappers: Vec<String>,
    /// Verbose runner logging.
    #[serde(default, skip_serializing_if = "is_false")]
    pub debug: bool,
    /// `wine` only: a DXVK directory with `x64/` (and optionally `x32/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dxvk: Option<PathBuf>,
    /// `wine` only: a VKD3D-Proton directory with `x64/` (and optionally
    /// `x86/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vkd3d: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Merged into `WINEDLLOVERRIDES`, e.g. `{ winhttp = "n,b" }`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dll_overrides: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wine: Option<WineSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub umu: Option<UmuSection>,
    /// The `proton` runner's build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<ProtonSection>,
    /// The `proton` runner's Steam integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam: Option<SteamIntegration>,
    #[serde(default, skip_serializing_if = "Anticheat::is_off")]
    pub anticheat: Anticheat,
    /// Where each game's compiled shaders go. Unset leaves it to the drivers
    /// and the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader_cache: Option<ShaderCache>,
    /// `slr` only, and required there: the Steam Linux Runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slr: Option<SlrSection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    /// A Windows executable through Wine.
    Wine,
    /// A Windows executable through Proton, outside Steam (`umu-run`).
    Umu,
    /// Steam's Proton (`proton run`), as Steam launches it.
    Proton,
    /// A Linux executable as is.
    Native,
    /// A Linux executable with a Steam Linux Runtime (`[slr]`, sruman).
    Slr,
}

impl Runner {
    pub const ALL: [Self; 5] = [Self::Wine, Self::Umu, Self::Proton, Self::Native, Self::Slr];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wine => "wine",
            Self::Umu => "umu",
            Self::Proton => "proton",
            Self::Native => "native",
            Self::Slr => "slr",
        }
    }

    /// Whether the runner runs Windows executables (through Wine).
    #[must_use]
    pub fn is_windows(self) -> bool {
        matches!(self, Self::Wine | Self::Umu | Self::Proton)
    }
}

impl fmt::Display for Runner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown runner `{0}`; use `wine`, `umu`, `proton`, `native` or `slr`")]
pub struct ParseRunnerError(pub String);

impl FromStr for Runner {
    type Err = ParseRunnerError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|runner| runner.as_str() == text)
            .ok_or_else(|| ParseRunnerError(text.to_owned()))
    }
}

/// `[wine]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WineSection {
    /// The Wine binary, or a Wine build directory with `bin/wine`. Unset:
    /// `wine` from `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// `[umu]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UmuSection {
    /// Unset: `umu-run` from `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub umu_run: Option<PathBuf>,
    /// The Proton build directory, passed as `PROTONPATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<PathBuf>,
    /// The default `GAMEID` (for protonfixes); a game's own wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    /// `STORE`, for protonfixes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
}

/// `[proton]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtonSection {
    /// A Steam-installed build name (`experimental`, `9.0`, ...) or the path
    /// of a Proton directory. Unset: the default build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

impl ProtonSection {
    /// Whether `build` names a directory rather than a Steam build.
    #[must_use]
    pub fn build_is_path(&self) -> bool {
        self.build
            .as_deref()
            .is_some_and(|build| build.starts_with('~') || build.contains('/'))
    }
}

/// `steam`: whether a Proton launch talks to the host Steam client.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteamIntegration {
    /// Bridge to the host Steam client when it is running.
    #[default]
    Auto,
    /// Always bridge, starting the host Steam client when needed (Steamworks
    /// DRM, Family Sharing).
    Host,
    /// Never bridge: the game sees no Steam client.
    None,
}

impl SteamIntegration {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Host => "host",
            Self::None => "none",
        }
    }
}

impl fmt::Display for SteamIntegration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `[anticheat]`: Proton's anti-cheat runtimes; both off by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anticheat {
    #[serde(default)]
    pub battleye: bool,
    #[serde(default)]
    pub eac: bool,
}

impl Anticheat {
    #[must_use]
    pub fn is_off(&self) -> bool {
        !self.battleye && !self.eac
    }
}

/// `[slr]`: which Steam Linux Runtime an `slr` profile uses, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlrSection {
    /// A runtime name (`sniper`), or `name=build` to pin a build. Its syntax
    /// and the known runtimes are the runtime manager's (sruman) to check.
    pub runtime: String,
    /// Where the game's libraries come from.
    #[serde(default, skip_serializing_if = "SlrMode::is_default")]
    pub mode: SlrMode,
    /// `container` mode: extra host paths shared read-write with the container.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filesystems_rw: Vec<PathBuf>,
    /// `container` mode: extra host paths shared read-only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filesystems_ro: Vec<PathBuf>,
    /// `overlay` mode: which copy of a library wins, by file name
    /// (`libSDL2-2.0.so.0`), over the runtime manager's defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lib_overrides: BTreeMap<String, LibOverride>,
}

impl SlrSection {
    pub fn new(runtime: impl Into<String>) -> Self {
        Self {
            runtime: runtime.into(),
            mode: SlrMode::default(),
            filesystems_rw: Vec::new(),
            filesystems_ro: Vec::new(),
            lib_overrides: BTreeMap::new(),
        }
    }
}

/// `slr.mode`: where an `slr` game's libraries come from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlrMode {
    /// The runtime's own system in a container (pressure-vessel): the host's
    /// libraries aren't used, except the graphics drivers.
    #[default]
    Container,
    /// No container: the host system and home, with the runtime's libraries
    /// found first (`LD_LIBRARY_PATH`). The host's C library and graphics
    /// drivers always stay.
    Overlay,
}

impl SlrMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Overlay => "overlay",
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl fmt::Display for SlrMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `slr.lib_overrides` value: which copy of a library an `overlay` game
/// loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibOverride {
    /// The runtime's copy.
    Runtime,
    /// The host's copy.
    Host,
    /// Whichever copy is newer.
    Newest,
}

impl LibOverride {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Host => "host",
            Self::Newest => "newest",
        }
    }
}

impl fmt::Display for LibOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `shader_cache`: `"off"`, or a directory that holds one folder per game.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderCache {
    /// Set nothing: DXVK, VKD3D-Proton and the drivers use their defaults.
    Off,
    /// Each game's caches go in `<dir>/<game>`.
    Dir(PathBuf),
}

impl ShaderCache {
    /// `off` (any case, surrounding blanks ignored) or a directory.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("off") {
            Self::Off
        } else {
            Self::Dir(PathBuf::from(text))
        }
    }

    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Off => "off".to_owned(),
            Self::Dir(dir) => dir.display().to_string(),
        }
    }

    /// The variables that put one game's shader caches in `dir`, in the
    /// layout Proton uses for its own: DXVK and VKD3D-Proton (Windows runners
    /// only), Mesa, NVIDIA, and AMD's own Vulkan driver.
    #[must_use]
    pub fn env(dir: &Path, runner: Runner) -> Vec<(&'static str, PathBuf)> {
        let mut env = Vec::with_capacity(5);
        if runner.is_windows() {
            env.push(("DXVK_STATE_CACHE_PATH", dir.join("DXVK_state_cache")));
            env.push(("VKD3D_SHADER_CACHE_PATH", dir.join("VKD3D_shader_cache")));
        }
        env.push(("MESA_SHADER_CACHE_DIR", dir.to_path_buf()));
        env.push(("__GL_SHADER_DISK_CACHE_PATH", dir.join("nvidiav1")));
        env.push(("AMD_VK_PIPELINE_CACHE_PATH", dir.join("AMDv1")));
        env
    }

    /// Creates the directories [`Self::env`] names: not every driver creates
    /// its own.
    ///
    /// # Errors
    ///
    /// The first directory that can't be created.
    pub fn create_dirs(dir: &Path, runner: Runner) -> std::io::Result<()> {
        for (_, path) in Self::env(dir, runner) {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

impl Serialize for ShaderCache {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_text())
    }
}

impl<'de> Deserialize<'de> for ShaderCache {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.trim().is_empty() {
            return Err(serde::de::Error::custom(
                "`shader_cache` is `off` or a directory",
            ));
        }
        Ok(Self::parse(&text))
    }
}

impl Profile {
    /// A profile with only `name` and `runner` set.
    pub fn new(name: impl Into<String>, runner: Runner) -> Self {
        Self {
            name: name.into(),
            runner,
            prefix: None,
            runner_args: Vec::new(),
            wrappers: Vec::new(),
            debug: false,
            dxvk: None,
            vkd3d: None,
            env: BTreeMap::new(),
            dll_overrides: BTreeMap::new(),
            wine: None,
            umu: None,
            proton: None,
            steam: None,
            anticheat: Anticheat::default(),
            shader_cache: None,
            slr: None,
        }
    }

    /// Whether `prefix` and `runner_args` mean something for this profile:
    /// every runner but `native`.
    #[must_use]
    pub fn uses_prefix(&self) -> bool {
        self.runner != Runner::Native
    }

    /// `prefix` with `~` expanded, when the runner uses one.
    #[must_use]
    pub fn prefix_path(&self, home: &Path) -> Option<PathBuf> {
        self.prefix
            .as_deref()
            .filter(|_| self.uses_prefix())
            .map(|prefix| crate::expand_tilde(prefix, home))
    }

    /// Checks the profile as a file: its name, that every path the runner
    /// uses is absolute (`~` expands to `home`), values no process can be
    /// started with, and settings the runner ignores (warnings).
    ///
    /// It doesn't check what only a launch needs (a supported runner, a
    /// `prefix` that is required, files that exist): that's each tool's.
    #[must_use]
    pub fn validate(&self, home: &Path) -> Vec<Issue> {
        let mut issues = Issues::new(Subject::Profile(self.name.clone()));
        let runner = self.runner;
        let ignored = |field: &str| format!("`{field}` is ignored for runner `{runner}`");
        issues.name(&self.name);

        if let Some(prefix) = &self.prefix {
            if !self.uses_prefix() {
                issues.warn(ignored("prefix"));
            } else if let Some(prefix) = issues.absolute("prefix", prefix, home)
                && home.starts_with(&prefix)
            {
                issues.error(format!(
                    "`prefix` {} is or contains the home directory; give it a directory of its own",
                    prefix.display()
                ));
            }
        }
        if !self.runner_args.is_empty() && !self.uses_prefix() {
            issues.warn(ignored("runner_args"));
        }
        issues.args("runner_args", &self.runner_args, true);
        issues.args("wrappers", &self.wrappers, false);
        issues.env("env", &self.env);

        if let Some(shader_cache) = &self.shader_cache {
            issues.shader_cache("shader_cache", shader_cache, home);
        }

        self.check_runner_settings(home, &mut issues);
        issues.finish()
    }

    /// The settings of one runner: checked for that runner, ignored (with a
    /// warning) for the others.
    fn check_runner_settings(&self, home: &Path, issues: &mut Issues) {
        let runner = self.runner;
        let ignored = |field: &str| format!("`{field}` is ignored for runner `{runner}`");

        for (field, path) in [("dxvk", &self.dxvk), ("vkd3d", &self.vkd3d)] {
            let Some(path) = path else { continue };
            match runner {
                Runner::Wine => {
                    issues.absolute(field, path, home);
                }
                Runner::Umu | Runner::Proton => issues.warn(format!(
                    "{}: Proton ships its own; use another Proton build instead",
                    ignored(field)
                )),
                Runner::Native | Runner::Slr => issues.warn(ignored(field)),
            }
        }

        if runner.is_windows() {
            for (dll, mode) in &self.dll_overrides {
                if let Err(message) = validate_dll_override(dll, mode) {
                    issues.error(message);
                }
            }
        } else {
            if !self.dll_overrides.is_empty() {
                issues.warn(ignored("dll_overrides"));
            }
            if !self.anticheat.is_off() {
                issues.warn(ignored("[anticheat]"));
            }
        }

        match (&self.wine, runner) {
            (Some(wine), Runner::Wine) => {
                if let Some(path) = &wine.path {
                    issues.absolute("wine.path", path, home);
                }
            }
            (Some(_), _) => issues.warn(ignored("[wine]")),
            (None, _) => {}
        }

        match (&self.umu, runner) {
            (Some(umu), Runner::Umu) => {
                if let Some(path) = &umu.umu_run {
                    issues.absolute("umu.umu_run", path, home);
                }
                if let Some(path) = &umu.proton {
                    issues.absolute("umu.proton", path, home);
                }
                for (field, value) in [("umu.game_id", &umu.game_id), ("umu.store", &umu.store)] {
                    if value
                        .as_deref()
                        .is_some_and(|value| value.trim().is_empty())
                    {
                        issues.error(format!("`{field}` must not be empty"));
                    }
                }
            }
            (Some(_), _) => issues.warn(ignored("[umu]")),
            (None, _) => {}
        }

        match (&self.proton, runner) {
            (Some(proton), Runner::Proton) => match proton.build.as_deref() {
                Some(build) if build.trim().is_empty() => {
                    issues.error("`proton.build` must not be empty");
                }
                Some(build) if proton.build_is_path() => {
                    issues.absolute("proton.build", Path::new(build), home);
                }
                _ => {}
            },
            (Some(_), _) => issues.warn(ignored("[proton]")),
            (None, _) => {}
        }
        if self.steam.is_some() && runner != Runner::Proton {
            issues.warn(ignored("steam"));
        }

        match (&self.slr, runner) {
            (Some(slr), Runner::Slr) => self.check_slr(slr, home, issues),
            (Some(_), _) => issues.warn(ignored("[slr]")),
            (None, Runner::Slr) => {
                issues.error("`[slr]` is required for runner `slr`: it names the runtime");
            }
            (None, _) => {}
        }
    }

    /// `[slr]` of an `slr` profile, and the settings its mode ignores.
    fn check_slr(&self, slr: &SlrSection, home: &Path, issues: &mut Issues) {
        if slr.runtime.trim().is_empty() {
            issues.error("`slr.runtime` must not be empty");
        }
        let mode = slr.mode;
        let ignored = |field: &str| format!("`{field}` is ignored in `{mode}` mode");
        match mode {
            SlrMode::Container => {
                for (field, paths) in [
                    ("slr.filesystems_rw", &slr.filesystems_rw),
                    ("slr.filesystems_ro", &slr.filesystems_ro),
                ] {
                    for path in paths {
                        issues.absolute(field, path, home);
                    }
                }
                if !slr.lib_overrides.is_empty() {
                    issues.warn(format!(
                        "{}: the container has the runtime's libraries only",
                        ignored("slr.lib_overrides")
                    ));
                }
            }
            SlrMode::Overlay => {
                for (field, paths) in [
                    ("slr.filesystems_rw", &slr.filesystems_rw),
                    ("slr.filesystems_ro", &slr.filesystems_ro),
                ] {
                    if !paths.is_empty() {
                        issues.warn(format!(
                            "{}: there's no container, so the game sees every path",
                            ignored(field)
                        ));
                    }
                }
                if self.prefix.is_some() {
                    issues.warn(
                        "in `overlay` mode, `prefix` only redirects `HOME` and `XDG_*`: a game \
                         that finds its home another way still reaches the real one",
                    );
                }
                for lib in slr.lib_overrides.keys() {
                    if let Err(message) = validate_lib_name(lib) {
                        issues.error(message);
                    }
                }
            }
        }
    }
}

/// Checks a `slr.lib_overrides` key: a library file name such as
/// `libSDL2-2.0.so.0`.
///
/// # Errors
///
/// What's wrong with the name, naming it.
// Library file names are case-sensitive: the loader only finds `.so`.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub fn validate_lib_name(lib: &str) -> Result<(), String> {
    if lib.is_empty()
        || lib.contains(['/', '\0', '=', ' '])
        || !(lib.ends_with(".so") || lib.contains(".so."))
    {
        return Err(format!(
            "`slr.lib_overrides` key `{lib}` is not a library file name (like `libSDL2-2.0.so.0`)"
        ));
    }
    Ok(())
}

/// Checks one `dll_overrides` entry: a DLL name, and Wine's override modes
/// (comma-separated `n`/`b`/`native`/`builtin`, or empty to disable the DLL).
///
/// # Errors
///
/// What's wrong with the entry, naming it.
pub fn validate_dll_override(dll: &str, mode: &str) -> Result<(), String> {
    if dll.is_empty() || dll.contains(['=', ',', ';', ' ', '\0']) {
        return Err(format!(
            "`dll_overrides` key `{dll}` is not a valid DLL name"
        ));
    }
    if mode.is_empty()
        || mode
            .split(',')
            .all(|part| matches!(part, "n" | "b" | "native" | "builtin"))
    {
        Ok(())
    } else {
        Err(format!(
            "`dll_overrides.{dll}` = `{mode}`: use `n`, `b`, `n,b`, `b,n` or `` (disabled)"
        ))
    }
}

// serde's `skip_serializing_if` passes a reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !value
}
