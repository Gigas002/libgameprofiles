# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- Example profiles for every runner (`examples/profiles`), checked by a test.

## [0.1.0]

- Initial release: the profile model, validation and store of lootrs
  (`liblootrs::model`, `liblootrs::store`), with sruman's `[slr]` section,
  unknown-key warnings and gaki's `shader_cache` check merged in.
- Audited (`docs/AUDIT.md`): atomic saves safe across threads and durable,
  written through symlinks with permissions kept; NUL characters, bad `env`
  keys and a `prefix` holding the home directory are errors; settings for
  another runner and unknown keys are warnings.
