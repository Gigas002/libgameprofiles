# Audit

Audit of libgameprofiles 0.1.0 before its first release, done on 2026-11-22
while it was extracted from lootrs (`liblootrs::model`, `liblootrs::store`)
and merged with sruman's copy of the model (`libsruman::model`) and gaki's
profile checks.

## Scope

All of `src/`: the schema (`profile`), validation, the issue types (`issue`)
and the profile directory store (`store`). The library has no `unsafe`, no
network access, no process spawning and reads no environment variables. Its
only side effects are file reads in `store::load*`, file writes in
`store::save`/`store::write_atomic`/`store::remove`, and directory creation in
`ShaderCache::create_dirs`.

## Gates

| Check                                                     | Result                                                 |
| --------------------------------------------------------- | ------------------------------------------------------ |
| `cargo test` (42 unit tests, 1 example test, 1 doctest)   | pass                                                   |
| `cargo clippy --all-targets -- -D warnings`               | clean                                                  |
| `cargo clippy --lib -- -D warnings -W clippy::pedantic`   | clean                                                  |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`          | clean                                                  |
| `cargo deny check` (advisories, bans, licenses, sources)  | ok                                                     |
| `typos`                                                   | clean                                                  |
| `cargo llvm-cov`                                          | 97.7% lines, 98.9% functions, 96.8% regions            |
| `cargo package`                                           | builds from the packaged sources                       |

Uncovered lines are I/O error branches that need a failing disk (a failed
`sync_all`, `read_dir` entry errors) and one defensive branch for a path
without a parent.

Dependencies: `serde`, `toml` and `thiserror` (all MIT/Apache-2.0),
`tempfile` for tests only.

## Findings

Fixed before release:

1. **Temporary-file collision between threads (data loss).** The temporary
   file was named `<file>.tmp.<pid>`, so two threads saving the same profile
   wrote the same temporary file: one rename failed, or the file got the other
   thread's half-written text. A per-process counter is now part of the name.
   `concurrent_saves_never_clobber_each_other` saves from 8 threads at once
   and reloads after every write; it failed before the fix.
2. **Saves weren't durable across a crash.** The rename is now followed by a
   (best-effort) `fsync` of the directory, after the file's own `fsync`.
3. **Saving replaced symlinks with files.** A profile directory of links into
   a dotfiles checkout lost its links on the first save. Writes now go through
   symlinks, also dangling ones, up to 40 links (Linux's own limit); a loop is
   an error, not a hang (`a_symlink_loop_is_an_error`).
4. **Saving reset permissions.** A `0600` profile (it may hold tokens in
   `env`) became `0644`. The existing mode is kept.
5. **A failed write left its temporary file behind.** It is removed now
   (`a_failed_write_leaves_nothing_behind`).
6. **NUL characters** in `env`, `runner_args`, `wrappers` or paths passed
   validation and then failed at spawn time with an unhelpful error. They are
   validation errors now, as are `env` keys that are empty or contain `=`.
7. **A `prefix` that is or contains the home directory** (`~`, `/home`, `/`)
   was accepted. Tools create, populate and clean up prefixes, so this is now
   an error. The check is lexical (after `~` expansion), not through symlinks:
   a symlink to `$HOME` used as a prefix isn't caught.
8. **Settings for another runner were silently ignored** (`[proton]`, `steam`
   or `[slr]` on a `wine` profile). They are kept, so switching runners loses
   nothing, and now produce a warning.
9. **Unknown keys were silently dropped by lootrs and gaki** (sruman warned).
   All three warn now, from one list (`KNOWN_KEYS`, tested against the schema
   in `every_known_key_is_a_field`).

Checked and fine:

- **Path traversal.** Profile names are `[A-Za-z0-9._-]+`, not `.`/`..`, not
  starting with `-`, so a name can't leave the profile directory or be read as
  an option. `save` and `remove` check the name before touching the disk
  (`names_that_leave_the_directory_are_refused`). `load` requires the `name`
  inside a file to match its file stem.
- **Hostile files.** Parsing is `toml`'s, which limits nesting; a deeply
  nested file is a parse error, not a stack overflow
  (`deep_nesting_is_an_error_not_a_crash`). A broken file never stops a
  directory from loading.
- **Crash leftovers.** Temporary files don't end in `.toml`, so the loader
  never picks one up.
- **Round trip.** Every field survives `save` → `load`, and defaults aren't
  written (`every_field_round_trips`, `defaults_are_not_written`).
- **`slr` settings** (added before release, reviewed the same way):
  `slr.mode` and the `lib_overrides` values are closed lists, so a typo is a
  parse error naming the line, never a silently different behaviour. A
  `lib_overrides` key must be a bare library file name (`*.so` or `*.so.*`, no
  `/`, space, `=` or NUL), so it can't name a path outside the runtime and
  host library folders; which libraries may be overridden at all (never the
  C library) is sruman's to enforce. Settings a mode ignores warn and aren't
  checked, like those of another runner.

## API and semver notes

- `Profile` and its sections are plain structs with public fields, and the
  enums are exhaustive, so tools can build and match them directly. The price
  is that a new field or variant is a breaking change: while the version is
  `0.x`, it means a minor bump (`0.1` → `0.2`).
- `Issue<S>` and `Issues<S>` are generic over the subject, so each tool keeps
  its own subjects (gaki's games have an app id) and converts the library's
  with `Issue::map_subject` and `From`.
- `runner` is required in every file. This keeps a file's meaning
  independent of the tool that reads it. `slr` is a runner of its own rather
  than `native` plus `[slr]`, so no runner changes meaning with an optional
  section, and each tool refuses the runners it can't launch.
- The fit of the API was checked by porting all three consumers to it
  (lootrs, gaki and sruman on their `feature/libgameprofiles` branches). Their
  test suites pass in every feature set, and lootrs and gaki report
  identically on real configuration directories before and after the port.

## Behaviour changes for the tools

Compared with the code each tool had before:

- **All:** findings 6 to 9 above.
- **lootrs:** `native` games get no DXVK/VKD3D shader cache variables (only
  Mesa, NVIDIA and AMD); `slr` profiles are refused (sruman runs them).
- **gaki:** `slr` profiles are refused.
- **sruman:** profile files need `runner = "slr"`; a malformed
  `slr.runtime` is a validation error instead of a load error.
