# libgameprofiles

Game profiles shared by [lootrs](https://github.com/Gigas002/lootrs),
[gaki](https://github.com/Gigas002/gaki) and
[sruman](https://github.com/Gigas002/sruman): the profile file format, its
validation, and loading and saving profile directories.

A profile says how games run: which runner, in which prefix, with which
environment and wrappers. Each tool reads the same `profiles/<name>.toml`
files, so one profile directory can serve all three. Each tool keeps its own
games list (`games.toml`).

| Runner   | What it runs                                                         | Launched by |
| -------- | -------------------------------------------------------------------- | ----------- |
| `wine`   | A Windows executable through Wine                                    | lootrs      |
| `umu`    | A Windows executable through Proton, outside Steam (`umu-run`)       | lootrs      |
| `proton` | A Steam game through Steam's own Proton (`proton run`)               | gaki        |
| `native` | A Linux executable as is                                             | lootrs, gaki |
| `slr`    | A Linux executable with a Steam Linux Runtime                        | sruman      |

## Profile files

`profiles/<name>.toml`; `name` must equal the file name without `.toml`, and
use only letters, digits, `.`, `_` and `-`. Paths are absolute or start with
`~`.

```toml
name = "default"
runner = "wine"                  # wine | umu | proton | native | slr
prefix = "~/prefixes/default"    # Wine prefix; see below for other runners
runner_args = []                 # between the runner and the executable
wrappers = ["mangohud"]          # before the whole command
debug = false                    # verbose runner logging
shader_cache = "~/.cache/shaders" # one folder per game, or "off"
dxvk = "~/dxvk/dxvk-2.7"         # wine: x64/ (and x32/) DLLs to install
vkd3d = "~/dxvk/vkd3d-proton-2.14" # wine: x64/ (and x86/) DLLs to install

[env]
DXVK_HUD = "fps"

[dll_overrides]                  # merged into WINEDLLOVERRIDES
winhttp = "n,b"                  # n, b, n,b, b,n, or "" to disable

[wine]
path = "/opt/wine-tkg"           # a wine binary or a build with bin/wine

[anticheat]                      # Proton's anti-cheat runtimes
battleye = false
eac = false
```

Runner-specific sections:

```toml
[umu]                            # runner = "umu"
umu_run = "/usr/bin/umu-run"     # default: umu-run from PATH
proton = "~/proton/GE-Proton10-25" # PROTONPATH
game_id = "umu-default"          # GAMEID; a game's own wins
store = "gog"                    # STORE

[proton]                         # runner = "proton"
build = "experimental"           # a Steam build name, or a Proton directory
# steam = "auto" | "host" | "none": bridge to the host Steam client

[slr]                            # runner = "slr"; required there
runtime = "steamrt4"             # the latest; or "steamrt4=<build>" to pin a build
mode = "container"               # container | overlay (see below)
filesystems_rw = ["/mnt/games"]  # container: extra host paths it sees
filesystems_ro = []

[slr.lib_overrides]              # overlay: which copy of a library wins
"libSDL2-2.0.so.0" = "host"      # runtime | host | newest
```

An `slr` profile has two independent choices:

- **The home:** with `prefix`, the game's home is `<prefix>/home`, so its
  saves and settings stay there, like a Wine prefix. Without it, the game uses
  your real home.
- **The libraries** (`mode`): `container` (the default) runs the game in the
  runtime's container, with the runtime's libraries only (plus your graphics
  drivers). `overlay` runs it on your system without a container, with the
  runtime's libraries added where yours are missing or older; your C library
  and graphics drivers always stay, and `lib_overrides` picks the copy for
  other libraries. In overlay
  mode, a `prefix` only redirects `HOME` and `XDG_*`, so it is leakier.

A commented example for each runner is in
[examples/profiles](examples/profiles); a test keeps them free of errors and
warnings.

What `prefix` and `runner_args` mean depends on the runner:

| Runner                | `prefix`                                  | `runner_args`                       |
| --------------------- | ----------------------------------------- | ----------------------------------- |
| `wine`, `umu`         | The Wine prefix                           | Between `wine`/`umu-run` and the exe |
| `proton`              | `STEAM_COMPAT_DATA_PATH`                  | Between `proton run` and the exe    |
| `slr`                 | The game's private home is `<prefix>/home` | Options of the runtime's entry point |
| `native`              | Ignored                                   | Ignored                             |

A setting that doesn't apply to the profile's runner is kept, so switching
`runner` back and forth loses nothing, and validation warns that it is
ignored. Keys the format doesn't know are warnings too, so a file written by a
newer tool still loads everywhere.

## Validation

`Profile::validate` checks a profile as a file, the same way in every tool:

- **Errors:** an invalid `name`; a relative path; a `prefix` that is or holds
  the home directory; a `shader_cache` directory that can't be created; a
  malformed `dll_overrides` entry; a `lib_overrides` key that isn't a library
  file name; an `slr` profile without `[slr]`; empty `wrappers`,
  `proton.build`, `slr.runtime`, `umu.game_id` or `umu.store`; environment variable names a
  process can't have; NUL characters in anything passed to a process.
- **Warnings:** settings the runner or the `slr` mode ignores (only those
  that are used are checked further), a `prefix` in `overlay` mode, and
  unknown keys (when loading).

What a launch needs is each tool's to check: lootrs requires a `prefix` and
`[umu].proton` and refuses `proton` and `slr`; gaki launches only `proton`
and `native`; sruman launches only `slr` and checks the runtime is known and
installed.

## Usage

```toml
[dependencies]
libgameprofiles = "0.1"
```

```rust,no_run
use std::path::Path;

use libgameprofiles::{Profile, Runner, store};

let home = Path::new("/home/user");
let dir = home.join(".config/lootrs/profiles");

let loaded = store::load_dir(&dir);
for issue in loaded.issues() {
    eprintln!("{issue}");
}
for profile in &loaded.profiles {
    for issue in profile.validate(home) {
        eprintln!("{issue}");
    }
}

let mut profile = Profile::new("native", Runner::Native);
profile.wrappers = vec!["gamemoderun".into()];
store::save(&dir, &profile).expect("saved");
```

Saving is atomic (a synced temporary file renamed over the old one), writes
through symlinks so a profile directory of links into a dotfiles checkout
keeps its links, and keeps an existing file's permissions.

The library was audited before its first release; findings and checks are in
[docs/AUDIT.md](docs/AUDIT.md).

## License

GPL-3.0-or-later, the same as lootrs and sruman.
