//! Integration tests for configuration parsing, overrides, and validation.

use std::{fs, path::Path};

use actioneer::{
    cli::ConfigArgs,
    config::{
        ActioneerConfig, DurationUnit, OutputMode, PinMode, RelativeDuration, UpdateLevel,
        find_config, load, load_config,
    },
};

fn write_github_config(root: &Path, content: &str) {
    let github = root.join(".github");
    fs::create_dir_all(&github).unwrap();
    fs::write(github.join("actioneer.toml"), content).unwrap();
}

#[test]
fn defaults_when_no_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = load(dir.path()).unwrap();

    assert_eq!(cfg.pin, PinMode::Sha);
    assert_eq!(cfg.update, UpdateLevel::Minor);
    assert!(!cfg.skip_branches);
    assert!(cfg.min_release_age.is_none());
    assert!(!cfg.offline);
    assert!(!cfg.no_cache);
    assert!(cfg.mode.is_none());
}

#[test]
fn find_config_returns_none_for_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    assert!(find_config(dir.path()).is_none());
}

#[test]
fn find_config_not_found_outside_github() {
    let dir = tempfile::tempdir().unwrap();
    // A config directly in the root (not under .github) should NOT be found.
    fs::write(dir.path().join("actioneer.toml"), "").unwrap();
    assert!(find_config(dir.path()).is_none());
}

#[test]
fn find_config_returns_path_when_exists() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), "");
    let found = find_config(dir.path());
    assert!(found.is_some());
    assert!(found.unwrap().ends_with(".github/actioneer.toml"));
}

#[test]
fn load_config_all_fields() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(
        dir.path(),
        r#"
pin = "sha"
update = "major"
skip_branches = true
min-release-age = "7d"
"#,
    );

    let cfg = load(dir.path()).unwrap();
    assert_eq!(cfg.pin, PinMode::Sha);
    assert_eq!(cfg.update, UpdateLevel::Major);
    assert!(cfg.skip_branches);
    let age = cfg.min_release_age.unwrap();
    assert_eq!(age.amount, 7);
    assert_eq!(age.unit, DurationUnit::Days);
}

#[test]
fn load_config_partial_falls_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), r#"pin = "tag""#);

    let cfg = load(dir.path()).unwrap();
    assert_eq!(cfg.pin, PinMode::Tag);
    assert_eq!(cfg.update, UpdateLevel::Minor); // default
    assert!(!cfg.skip_branches); // default
}

#[test]
fn load_config_empty_file_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), "");

    let cfg = load(dir.path()).unwrap();
    assert_eq!(cfg.pin, PinMode::Sha);
    assert_eq!(cfg.update, UpdateLevel::Minor);
}

#[test]
fn load_config_unknown_key_is_error() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), r#"unknown_key = "value""#);

    assert!(load_config(&dir.path().join(".github").join("actioneer.toml")).is_err());
}

#[test]
fn parse_duration_days() {
    let d: RelativeDuration = "30d".parse().unwrap();
    assert_eq!(d.amount, 30);
    assert_eq!(d.unit, DurationUnit::Days);
}

#[test]
fn parse_duration_hours() {
    let d: RelativeDuration = "4h".parse().unwrap();
    assert_eq!(d.amount, 4);
    assert_eq!(d.unit, DurationUnit::Hours);
}

#[test]
fn parse_duration_minutes() {
    let d: RelativeDuration = "10m".parse().unwrap();
    assert_eq!(d.amount, 10);
    assert_eq!(d.unit, DurationUnit::Minutes);
}

#[test]
fn parse_duration_display_roundtrip() {
    let original = "42h";
    let d: RelativeDuration = original.parse().unwrap();
    assert_eq!(d.to_string(), original);
}

#[test]
fn parse_duration_error_unknown_unit() {
    assert!("10x".parse::<RelativeDuration>().is_err());
}

#[test]
fn parse_duration_error_empty() {
    assert!("".parse::<RelativeDuration>().is_err());
}

#[test]
fn parse_duration_error_no_number() {
    assert!("d".parse::<RelativeDuration>().is_err());
}

#[test]
fn parse_pin_mode_valid() {
    assert_eq!("sha".parse::<PinMode>().unwrap(), PinMode::Sha);
    assert_eq!("tag".parse::<PinMode>().unwrap(), PinMode::Tag);
}

#[test]
fn parse_pin_mode_invalid() {
    assert!("commit".parse::<PinMode>().is_err());
}

#[test]
fn parse_update_level_valid() {
    assert_eq!("major".parse::<UpdateLevel>().unwrap(), UpdateLevel::Major);
    assert_eq!("minor".parse::<UpdateLevel>().unwrap(), UpdateLevel::Minor);
    assert_eq!("patch".parse::<UpdateLevel>().unwrap(), UpdateLevel::Patch);
}

#[test]
fn parse_update_level_invalid() {
    assert!("breaking".parse::<UpdateLevel>().is_err());
}

#[test]
fn apply_overrides_all_fields() {
    let mut cfg = ActioneerConfig::default();
    let args = ConfigArgs {
        pin: Some(PinMode::Tag),
        update: Some(UpdateLevel::Patch),
        skip_branches: Some(true),
        min_release_age: Some(RelativeDuration {
            amount: 5,
            unit: DurationUnit::Hours,
        }),
        ..Default::default()
    };
    cfg.apply_overrides(&args);

    assert_eq!(cfg.pin, PinMode::Tag);
    assert_eq!(cfg.update, UpdateLevel::Patch);
    assert!(cfg.skip_branches);
    assert_eq!(cfg.min_release_age.unwrap().amount, 5);
}

#[test]
fn apply_overrides_partial_leaves_rest_unchanged() {
    let mut cfg = ActioneerConfig::default();
    let args = ConfigArgs {
        pin: Some(PinMode::Tag),
        ..Default::default()
    };
    cfg.apply_overrides(&args);

    assert_eq!(cfg.pin, PinMode::Tag);
    assert_eq!(cfg.update, UpdateLevel::Minor); // unchanged
    assert!(!cfg.skip_branches); // unchanged
    assert!(cfg.min_release_age.is_none()); // unchanged
}

#[test]
fn cli_overrides_file_config() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), r#"pin = "sha""#);

    let mut cfg = load(dir.path()).unwrap();
    assert_eq!(cfg.pin, PinMode::Sha);

    let args = ConfigArgs {
        pin: Some(PinMode::Tag),
        ..Default::default()
    };
    cfg.apply_overrides(&args);
    assert_eq!(cfg.pin, PinMode::Tag);
}

#[test]
fn load_config_new_fields() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(
        dir.path(),
        r#"
offline = true
no_cache = false
mode = "plain"
"#,
    );

    let cfg = load(dir.path()).unwrap();
    assert!(cfg.offline);
    assert!(!cfg.no_cache);
    assert_eq!(cfg.mode, Some(OutputMode::Plain));
}

#[test]
fn load_config_mode_json() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), r#"mode = "json""#);
    let cfg = load(dir.path()).unwrap();
    assert_eq!(cfg.mode, Some(OutputMode::Json));
}

#[test]
fn load_config_mode_defaults_to_none() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), "");
    let cfg = load(dir.path()).unwrap();
    assert!(cfg.mode.is_none());
}

#[test]
fn validate_ok_when_neither_flag_set() {
    let cfg = ActioneerConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn validate_ok_offline_only() {
    let cfg = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    assert!(cfg.validate().is_ok());
}

#[test]
fn validate_ok_no_cache_only() {
    let cfg = ActioneerConfig {
        no_cache: true,
        ..Default::default()
    };
    assert!(cfg.validate().is_ok());
}

#[test]
fn validate_err_offline_and_no_cache() {
    let cfg = ActioneerConfig {
        offline: true,
        no_cache: true,
        ..Default::default()
    };
    let err = cfg.validate().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("offline") && msg.contains("no_cache"),
        "error message should mention both flags: {msg}"
    );
}

#[test]
fn validate_conflict_set_by_cli_override() {
    let dir = tempfile::tempdir().unwrap();
    write_github_config(dir.path(), "offline = true");

    let mut cfg = load(dir.path()).unwrap();
    let args = ConfigArgs {
        no_cache: Some(true),
        ..Default::default()
    };
    cfg.apply_overrides(&args);
    assert!(cfg.validate().is_err());
}

#[test]
fn apply_overrides_new_fields() {
    let mut cfg = ActioneerConfig::default();
    let args = ConfigArgs {
        offline: Some(true),
        no_cache: Some(false),
        mode: Some(OutputMode::Json),
        ..Default::default()
    };
    cfg.apply_overrides(&args);
    assert!(cfg.offline);
    assert!(!cfg.no_cache);
    assert_eq!(cfg.mode, Some(OutputMode::Json));
}

#[test]
fn parse_output_mode_valid() {
    assert_eq!("plain".parse::<OutputMode>().unwrap(), OutputMode::Plain);
    assert_eq!("json".parse::<OutputMode>().unwrap(), OutputMode::Json);
}

#[test]
fn parse_output_mode_invalid() {
    assert!("fancy".parse::<OutputMode>().is_err());
    assert!("tui".parse::<OutputMode>().is_err());
}

#[test]
fn output_mode_display_roundtrip() {
    for mode in [OutputMode::Plain, OutputMode::Json] {
        let s = mode.to_string();
        let parsed: OutputMode = s.parse().unwrap();
        assert_eq!(parsed, mode);
    }
}
