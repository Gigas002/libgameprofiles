# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.1]

- Same code as 0.1.0, released again from the reworded git history.

## [0.1.0]

- Initial release: the profile model, its validation (including unknown-key
  warnings and the `shader_cache` check), and the profile directory store.
- Audited (`docs/AUDIT.md`): atomic saves safe across threads and durable,
  written through symlinks with permissions kept; NUL characters, bad `env`
  keys and a `prefix` holding the home directory are errors; settings for
  another runner and unknown keys are warnings.
- `slr` is a runner of its own, which `[slr]` belongs to and requires, with
  `mode = "container" | "overlay"` and `[slr.lib_overrides]` for overlay mode.
- Example profiles for every runner (`examples/profiles`), checked by a test.
