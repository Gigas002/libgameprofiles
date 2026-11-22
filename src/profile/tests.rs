use std::{collections::BTreeMap, path::Path};

use super::*;
use crate::Severity;

const HOME: &str = "/home/u";

fn home() -> &'static Path {
    Path::new(HOME)
}

fn messages(issues: &[Issue], severity: Severity) -> Vec<&str> {
    issues
        .iter()
        .filter(|issue| issue.severity == severity)
        .map(|issue| issue.message.as_str())
        .collect()
}

fn errors(issues: &[Issue]) -> Vec<&str> {
    messages(issues, Severity::Error)
}

fn warnings(issues: &[Issue]) -> Vec<&str> {
    messages(issues, Severity::Warning)
}

fn wine() -> Profile {
    Profile {
        prefix: Some("~/prefixes/default".into()),
        ..Profile::new("default", Runner::Wine)
    }
}

fn slr() -> Profile {
    Profile {
        prefix: Some("/pfx/native".into()),
        runner_args: vec!["--verbose".into()],
        slr: Some(SlrSection::new("sniper")),
        ..Profile::new("sniper", Runner::Native)
    }
}

#[test]
fn minimal_profiles_are_valid() {
    for runner in Runner::ALL {
        assert_eq!(
            Profile::new("p", runner).validate(home()),
            Vec::new(),
            "{runner}"
        );
    }
    assert_eq!(wine().validate(home()), Vec::new());
    assert_eq!(slr().validate(home()), Vec::new());
}

#[test]
fn runner_names() {
    for runner in Runner::ALL {
        assert_eq!(runner.as_str().parse::<Runner>(), Ok(runner));
        assert_eq!(runner.to_string(), runner.as_str());
    }
    assert_eq!(
        "Wine".parse::<Runner>().unwrap_err().to_string(),
        "unknown runner `Wine`; use `wine`, `umu`, `proton` or `native`"
    );
    assert!(Runner::Proton.is_windows() && !Runner::Native.is_windows());
    for steam in [
        SteamIntegration::Auto,
        SteamIntegration::Host,
        SteamIntegration::None,
    ] {
        let text = toml::to_string(&Profile {
            steam: Some(steam),
            ..Profile::new("p", Runner::Proton)
        })
        .unwrap();
        assert!(text.contains(&format!("steam = \"{steam}\"")), "{text}");
    }
    assert_eq!(SteamIntegration::default(), SteamIntegration::Auto);
}

#[test]
fn every_field_round_trips() {
    let text = r#"
name = "all"
runner = "proton"
prefix = "~/pfx"
runner_args = ["-a"]
wrappers = ["mangohud", "gamemoderun"]
debug = true
dxvk = "/opt/dxvk"
vkd3d = "/opt/vkd3d"
shader_cache = "~/.cache/shaders"
steam = "host"

[env]
DXVK_HUD = "fps"

[dll_overrides]
winhttp = "n,b"
d3d9 = ""

[wine]
path = "/opt/wine"

[umu]
umu_run = "/usr/bin/umu-run"
proton = "/opt/GE-Proton"
game_id = "umu-1"
store = "gog"

[proton]
build = "experimental"

[anticheat]
battleye = true

[slr]
runtime = "sniper=3.0.1"
filesystems_rw = ["/mnt/games"]
filesystems_ro = ["/opt/data"]
"#;
    let profile: Profile = toml::from_str(text).unwrap();
    assert_eq!(profile.steam, Some(SteamIntegration::Host));
    assert_eq!(
        profile.anticheat,
        Anticheat {
            battleye: true,
            eac: false
        }
    );
    assert_eq!(profile.slr.as_ref().unwrap().runtime, "sniper=3.0.1");
    let back: Profile = toml::from_str(&toml::to_string_pretty(&profile).unwrap()).unwrap();
    assert_eq!(back, profile);
}

#[test]
fn defaults_are_not_written() {
    let text = toml::to_string_pretty(&Profile::new("p", Runner::Native)).unwrap();
    assert_eq!(text, "name = \"p\"\nrunner = \"native\"\n");
}

#[test]
fn runner_is_required_and_lowercase() {
    assert!(toml::from_str::<Profile>("name = \"p\"").is_err());
    assert!(toml::from_str::<Profile>("name = \"p\"\nrunner = \"Wine\"").is_err());
    assert!(toml::from_str::<Profile>("name = \"p\"\nrunner = \"native\"\n[slr]").is_err());
}

#[test]
fn relative_paths_are_errors() {
    let profile = Profile {
        prefix: Some("prefixes/default".into()),
        dxvk: Some("dxvk".into()),
        vkd3d: Some("~/vkd3d".into()),
        wine: Some(WineSection {
            path: Some("wine".into()),
        }),
        ..wine()
    };
    assert_eq!(
        errors(&profile.validate(home())),
        [
            "`prefix` must be an absolute path (or start with `~`), got `prefixes/default`",
            "`dxvk` must be an absolute path (or start with `~`), got `dxvk`",
            "`wine.path` must be an absolute path (or start with `~`), got `wine`",
        ]
    );

    let profile = Profile {
        umu: Some(UmuSection {
            umu_run: Some("umu-run".into()),
            proton: Some("GE-Proton".into()),
            ..UmuSection::default()
        }),
        ..Profile::new("u", Runner::Umu)
    };
    assert_eq!(errors(&profile.validate(home())).len(), 2);

    let mut profile = slr();
    profile.slr.as_mut().unwrap().filesystems_ro = vec!["data".into()];
    assert_eq!(
        errors(&profile.validate(home())),
        ["`slr.filesystems_ro` must be an absolute path (or start with `~`), got `data`"]
    );
}

#[test]
fn prefix_must_not_hold_the_home() {
    for prefix in ["~", "/home", "/", "~/"] {
        let profile = Profile {
            prefix: Some(prefix.into()),
            ..wine()
        };
        let errors = errors(&profile.validate(home()))
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{prefix}: {errors:?}");
        assert!(
            errors[0].contains("is or contains the home directory"),
            "{prefix}"
        );
    }
    let profile = Profile {
        prefix: Some("~/pfx".into()),
        ..wine()
    };
    assert!(errors(&profile.validate(home())).is_empty());
}

#[test]
fn native_uses_prefix_and_runner_args_only_with_slr() {
    let profile = Profile {
        prefix: Some("/pfx".into()),
        runner_args: vec!["-x".into()],
        ..Profile::new("n", Runner::Native)
    };
    assert!(!profile.uses_prefix());
    assert_eq!(profile.prefix_path(home()), None);
    assert_eq!(
        warnings(&profile.validate(home())),
        [
            "`prefix` is ignored for runner `native` without `[slr]`",
            "`runner_args` is ignored for runner `native` without `[slr]`",
        ]
    );
    assert!(slr().uses_prefix());
    assert_eq!(slr().prefix_path(home()), Some("/pfx/native".into()));
    assert_eq!(
        wine().prefix_path(home()),
        Some("/home/u/prefixes/default".into())
    );
}

#[test]
fn settings_of_other_runners_are_ignored_with_warnings() {
    let profile = Profile {
        dxvk: Some("/opt/dxvk".into()),
        wine: Some(WineSection::default()),
        umu: Some(UmuSection::default()),
        proton: Some(ProtonSection::default()),
        steam: Some(SteamIntegration::None),
        dll_overrides: BTreeMap::from([("d3d11".into(), "bogus".into())]),
        anticheat: Anticheat {
            battleye: true,
            eac: false,
        },
        slr: Some(SlrSection::new("")),
        ..Profile::new("n", Runner::Wine)
    };
    let issues = profile.validate(home());
    // The bogus override is checked: wine uses overrides. `[slr]` isn't.
    assert_eq!(
        errors(&issues),
        ["`dll_overrides.d3d11` = `bogus`: use `n`, `b`, `n,b`, `b,n` or `` (disabled)"]
    );
    assert_eq!(
        warnings(&issues),
        [
            "`[umu]` is ignored for runner `wine`",
            "`[proton]` is ignored for runner `wine`",
            "`steam` is ignored for runner `wine`",
            "`[slr]` is ignored for runner `wine`",
        ]
    );

    // An ignored setting isn't judged: a relative `dxvk` is fine for `native`.
    let native = Profile {
        runner: Runner::Native,
        dxvk: Some("relative".into()),
        slr: None,
        ..profile
    };
    let issues = native.validate(home());
    assert_eq!(errors(&issues), Vec::<&str>::new());
    assert_eq!(
        warnings(&issues),
        [
            "`dxvk` is ignored for runner `native`",
            "`dll_overrides` is ignored for runner `native`",
            "`[anticheat]` is ignored for runner `native`",
            "`[wine]` is ignored for runner `native`",
            "`[umu]` is ignored for runner `native`",
            "`[proton]` is ignored for runner `native`",
            "`steam` is ignored for runner `native`",
        ]
    );
}

#[test]
fn proton_ships_its_own_dxvk() {
    for runner in [Runner::Umu, Runner::Proton] {
        let profile = Profile {
            vkd3d: Some("/opt/vkd3d".into()),
            ..Profile::new("p", runner)
        };
        assert_eq!(
            warnings(&profile.validate(home())),
            [format!(
                "`vkd3d` is ignored for runner `{runner}`: Proton ships its own; use another \
                 Proton build instead"
            )]
        );
    }
}

#[test]
fn proton_build() {
    let build = |b: &str| ProtonSection {
        build: Some(b.into()),
    };
    assert!(build("/opt/proton").build_is_path());
    assert!(build("~/proton").build_is_path());
    assert!(build("dir/proton").build_is_path());
    assert!(!build("experimental").build_is_path());
    assert!(!ProtonSection::default().build_is_path());

    let check = |b: &str| {
        Profile {
            proton: Some(build(b)),
            ..Profile::new("p", Runner::Proton)
        }
        .validate(home())
    };
    assert!(check("experimental").is_empty());
    assert!(check("~/proton").is_empty());
    assert_eq!(errors(&check(" ")), ["`proton.build` must not be empty"]);
    assert_eq!(
        errors(&check("dir/proton")),
        ["`proton.build` must be an absolute path (or start with `~`), got `dir/proton`"]
    );
}

#[test]
fn empty_values_are_errors() {
    let profile = Profile {
        umu: Some(UmuSection {
            game_id: Some(String::new()),
            store: Some(" ".into()),
            ..UmuSection::default()
        }),
        ..Profile::new("u", Runner::Umu)
    };
    assert_eq!(
        errors(&profile.validate(home())),
        [
            "`umu.game_id` must not be empty",
            "`umu.store` must not be empty"
        ]
    );
    let mut profile = slr();
    profile.slr.as_mut().unwrap().runtime = " ".into();
    assert_eq!(
        errors(&profile.validate(home())),
        ["`slr.runtime` must not be empty"]
    );
}

#[test]
fn dll_override_modes() {
    for mode in ["n", "b", "n,b", "b,n", "native,builtin", ""] {
        assert_eq!(validate_dll_override("winhttp", mode), Ok(()), "{mode}");
    }
    for (dll, mode) in [
        ("winhttp", "x"),
        ("winhttp", "n,"),
        ("winhttp", "n, b"),
        ("", "n"),
        ("a=b", "n"),
        ("a b", "n"),
        ("a,b", "n"),
    ] {
        assert!(validate_dll_override(dll, mode).is_err(), "{dll}={mode}");
    }
}

#[test]
fn process_values() {
    let profile = Profile {
        env: BTreeMap::from([("A=B".into(), "1".into()), ("C".into(), "\0".into())]),
        wrappers: vec!["mangohud".into(), " ".into()],
        runner_args: vec!["a\0".into()],
        ..wine()
    };
    assert_eq!(
        errors(&profile.validate(home())),
        [
            "`runner_args` must not contain a NUL character",
            "`wrappers` must not contain empty entries",
            "`env` key `A=B` is not a valid variable name",
            "`env.C` must not contain a NUL character",
        ]
    );
}

#[test]
fn bad_name() {
    assert_eq!(
        errors(&Profile::new("my game", Runner::Native).validate(home())),
        ["name `my game` contains ` `; use only letters, digits, `.`, `_` and `-`"]
    );
}

#[test]
fn shader_cache_is_off_or_a_directory() {
    let parse = |text: &str| -> Profile {
        toml::from_str(&format!(
            "name = \"p\"\nrunner = \"native\"\nshader_cache = \"{text}\"\n"
        ))
        .unwrap()
    };
    assert_eq!(parse("off").shader_cache, Some(ShaderCache::Off));
    assert_eq!(parse(" OFF ").shader_cache, Some(ShaderCache::Off));
    assert_eq!(
        parse("~/cache").shader_cache,
        Some(ShaderCache::Dir("~/cache".into()))
    );
    for empty in ["", "  "] {
        assert!(
            toml::from_str::<Profile>(&format!(
                "name = \"p\"\nrunner = \"native\"\nshader_cache = \"{empty}\"\n"
            ))
            .is_err()
        );
    }
    // Written back the way it was read.
    let profile = parse("off");
    let text = toml::to_string(&profile).unwrap();
    assert!(text.contains("shader_cache = \"off\""), "{text}");
    assert_eq!(toml::from_str::<Profile>(&text).unwrap(), profile);
}

#[test]
fn shader_cache_directory_must_be_absolute_and_creatable() {
    let check = |dir: &Path| {
        Profile {
            shader_cache: Some(ShaderCache::Dir(dir.to_path_buf())),
            ..Profile::new("p", Runner::Native)
        }
        .validate(home())
    };
    assert_eq!(
        errors(&check(Path::new("cache"))),
        ["`shader_cache` must be an absolute path (or start with `~`), got `cache`"]
    );
    assert!(check(Path::new("~/cache")).is_empty());

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    assert!(check(&dir.path().join("new/cache")).is_empty());
    assert_eq!(
        errors(&check(&file.join("cache"))),
        [format!(
            "`shader_cache` can't be created: `{}` is not a directory",
            file.display()
        )]
    );
}

#[test]
fn shader_cache_env_follows_protons_layout() {
    let dir = Path::new("/c/g");
    let env: BTreeMap<_, _> = ShaderCache::env(dir, Runner::Wine).into_iter().collect();
    assert_eq!(
        env["DXVK_STATE_CACHE_PATH"],
        Path::new("/c/g/DXVK_state_cache")
    );
    assert_eq!(
        env["VKD3D_SHADER_CACHE_PATH"],
        Path::new("/c/g/VKD3D_shader_cache")
    );
    assert_eq!(env["MESA_SHADER_CACHE_DIR"], Path::new("/c/g"));
    assert_eq!(
        env["__GL_SHADER_DISK_CACHE_PATH"],
        Path::new("/c/g/nvidiav1")
    );
    assert_eq!(env["AMD_VK_PIPELINE_CACHE_PATH"], Path::new("/c/g/AMDv1"));

    // Native games have no DXVK or VKD3D-Proton.
    let native: Vec<_> = ShaderCache::env(dir, Runner::Native)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        native,
        [
            "MESA_SHADER_CACHE_DIR",
            "__GL_SHADER_DISK_CACHE_PATH",
            "AMD_VK_PIPELINE_CACHE_PATH"
        ]
    );

    let tmp = tempfile::tempdir().unwrap();
    ShaderCache::create_dirs(&tmp.path().join("g"), Runner::Umu).unwrap();
    assert!(tmp.path().join("g/DXVK_state_cache").is_dir());
    assert!(tmp.path().join("g/AMDv1").is_dir());
}
